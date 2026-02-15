"""LSP client implementation using JSON-RPC over stdio.

This module implements a client for the Cangjie Language Server Protocol
using JSON-RPC 2.0 over stdin/stdout.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlparse

from cangjie_mcp.lsp.config import LSPInitOptions
from cangjie_mcp.lsp.types import (
    CompletionItem,
    Diagnostic,
    DocumentSymbol,
    HoverResult,
    Location,
)
from cangjie_mcp.lsp.utils import is_dict, is_list

logger = logging.getLogger(__name__)


@dataclass
class CangjieClient:
    """Cangjie LSP client.

    Communicates with the Cangjie LSP server via JSON-RPC over stdio.
    """

    sdk_path: Path
    root_path: Path
    init_options: LSPInitOptions
    env: dict[str, str]
    args: list[str]

    process: subprocess.Popen[bytes] | None = field(default=None, init=False)
    _reader: asyncio.StreamReader | None = field(default=None, init=False)
    _writer: asyncio.StreamWriter | None = field(default=None, init=False)
    _request_id: int = field(default=0, init=False)
    _pending_requests: dict[int, asyncio.Future[Any]] = field(
        default_factory=lambda: dict[int, asyncio.Future[Any]](), init=False
    )
    _diagnostics: dict[str, list[Diagnostic]] = field(default_factory=lambda: dict[str, list[Diagnostic]](), init=False)
    _files: dict[str, int] = field(default_factory=lambda: dict[str, int](), init=False)  # path -> version
    _initialized: bool = field(default=False, init=False)
    _message_task: asyncio.Task[None] | None = field(default=None, init=False)

    @property
    def is_initialized(self) -> bool:
        """Check if the client has been initialized."""
        return self._initialized

    async def start(self, timeout: int = 45000) -> None:
        """Start the LSP server and initialize the connection.

        Args:
            timeout: Initialization timeout in milliseconds
        """
        # Build command
        exe = "LSPServer.exe" if sys.platform == "win32" else "LSPServer"
        cmd = [str(self.sdk_path / "tools" / "bin" / exe), *self.args]

        logger.info(f"Starting LSP server: {' '.join(cmd)}")

        # Start process
        self.process = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=self.env,
            cwd=str(self.root_path),
        )

        # Wrap as asyncio streams
        loop = asyncio.get_event_loop()
        self._reader = asyncio.StreamReader()
        protocol = asyncio.StreamReaderProtocol(self._reader)

        if self.process.stdout:
            await loop.connect_read_pipe(lambda: protocol, self.process.stdout)

        # Create writer
        if self.process.stdin:
            write_transport, write_protocol = await loop.connect_write_pipe(
                asyncio.streams.FlowControlMixin,
                self.process.stdin,
            )
            self._writer = asyncio.StreamWriter(
                write_transport,
                write_protocol,
                None,
                loop,
            )

        # Start message reader task
        self._message_task = asyncio.create_task(self._read_messages())

        # Send initialize request
        await self._initialize(timeout)

    async def _initialize(self, timeout: int) -> None:
        """Send LSP initialize request."""
        root_uri = self.root_path.as_uri()

        result = await asyncio.wait_for(
            self._send_request(
                "initialize",
                {
                    "processId": self.process.pid if self.process else None,
                    "rootUri": root_uri,
                    "rootPath": str(self.root_path),
                    "workspaceFolders": [{"name": "workspace", "uri": root_uri}],
                    "initializationOptions": self.init_options.model_dump(by_alias=True),
                    "capabilities": {
                        "textDocument": {
                            "synchronization": {
                                "dynamicRegistration": True,
                                "didSave": True,
                                "willSave": True,
                            },
                            "publishDiagnostics": {
                                "versionSupport": True,
                                "relatedInformation": True,
                            },
                            "hover": {
                                "contentFormat": ["markdown", "plaintext"],
                            },
                            "completion": {
                                "editsNearCursor": True,
                            },
                            "definition": {},
                            "references": {},
                            "documentSymbol": {
                                "hierarchicalDocumentSymbolSupport": True,
                            },
                        },
                        "workspace": {
                            "configuration": True,
                            "workspaceFolders": True,
                        },
                        "window": {
                            "workDoneProgress": True,
                        },
                    },
                },
            ),
            timeout=timeout / 1000,
        )

        result_dict = result if is_dict(result) else {}
        capabilities = result_dict.get("capabilities", {})
        logger.info(f"LSP server capabilities: {list(capabilities.keys())}")

        # Send initialized notification
        await self._send_notification("initialized", {})
        self._initialized = True
        logger.info("LSP client initialized successfully")

    async def _send_request(self, method: str, params: dict[str, Any]) -> Any:  # noqa: ANN401
        """Send JSON-RPC request and wait for response.

        Args:
            method: LSP method name
            params: Method parameters

        Returns:
            Response result (JSON-RPC can return any valid JSON type)
        """
        self._request_id += 1
        request_id = self._request_id

        message = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        }

        future: asyncio.Future[Any] = asyncio.get_event_loop().create_future()
        self._pending_requests[request_id] = future

        await self._send_message(message)
        return await future

    async def _send_notification(self, method: str, params: dict[str, Any]) -> None:
        """Send JSON-RPC notification (no response expected).

        Args:
            method: LSP method name
            params: Method parameters
        """
        message = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }
        await self._send_message(message)

    async def _send_message(self, message: dict[str, Any]) -> None:
        """Send JSON-RPC message over stdio.

        Args:
            message: Message to send
        """
        content = json.dumps(message)
        content_bytes = content.encode("utf-8")
        header = f"Content-Length: {len(content_bytes)}\r\n\r\n"

        if self._writer:
            self._writer.write(header.encode("utf-8") + content_bytes)
            await self._writer.drain()

    async def _read_messages(self) -> None:
        """Read and process incoming LSP messages."""
        while self._reader and not self._reader.at_eof():
            try:
                # Read headers
                headers: dict[str, str] = {}
                while True:
                    line = await self._reader.readline()
                    if line == b"\r\n":
                        break
                    if b":" in line:
                        key, value = line.decode("utf-8").strip().split(":", 1)
                        headers[key.strip()] = value.strip()

                # Read content
                content_length = int(headers.get("Content-Length", 0))
                if content_length > 0:
                    content = await self._reader.readexactly(content_length)
                    message = json.loads(content.decode("utf-8"))
                    await self._handle_message(message)
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error reading LSP message: {e}")
                break

    async def _handle_message(self, message: dict[str, Any]) -> None:
        """Handle incoming LSP message.

        Args:
            message: Parsed JSON-RPC message
        """
        if "id" in message and "method" not in message:
            # Response to our request
            request_id = message["id"]
            if request_id in self._pending_requests:
                future = self._pending_requests.pop(request_id)
                if "error" in message:
                    error = message["error"]
                    future.set_exception(Exception(f"LSP Error {error.get('code')}: {error.get('message')}"))
                else:
                    future.set_result(message.get("result"))
        elif "method" in message:
            # Server notification or request
            method = message["method"]
            params = message.get("params", {})

            if method == "textDocument/publishDiagnostics":
                await self._handle_diagnostics(params)
            elif method == "window/workDoneProgress/create":
                # Acknowledge progress request
                if "id" in message:
                    await self._send_message(
                        {
                            "jsonrpc": "2.0",
                            "id": message["id"],
                            "result": None,
                        }
                    )
            elif method == "workspace/configuration" and "id" in message:
                # Return empty configuration
                items = params.get("items", [])
                await self._send_message(
                    {
                        "jsonrpc": "2.0",
                        "id": message["id"],
                        "result": [{}] * len(items),
                    }
                )

    async def _handle_diagnostics(self, params: dict[str, Any]) -> None:
        """Handle diagnostics notification.

        Args:
            params: Diagnostics parameters
        """
        uri: str = params.get("uri", "")
        path: str = unquote(urlparse(uri).path)

        # Windows: remove leading slash from /C:/path
        if sys.platform == "win32" and path.startswith("/") and len(path) > 2 and path[2] == ":":
            path = path[1:]

        raw_diagnostics = params.get("diagnostics", [])
        self._diagnostics[path] = [Diagnostic.model_validate(d) for d in raw_diagnostics]
        logger.debug(f"Received {len(raw_diagnostics)} diagnostics for {path}")

    # =========================================================================
    # File Synchronization
    # =========================================================================

    async def open_file(self, file_path: str) -> None:
        """Notify server that a file is opened.

        Args:
            file_path: Absolute path to the file
        """
        path = Path(file_path)
        if not path.exists():
            raise FileNotFoundError(f"File not found: {file_path}")

        text = path.read_text(encoding="utf-8")
        uri = path.as_uri()

        version = self._files.get(file_path, 0)

        if file_path in self._files:
            # File already open, send change notification
            version += 1
            self._files[file_path] = version
            await self._send_notification(
                "textDocument/didChange",
                {
                    "textDocument": {"uri": uri, "version": version},
                    "contentChanges": [{"text": text}],
                },
            )
        else:
            # First open
            self._files[file_path] = 0
            await self._send_notification(
                "textDocument/didOpen",
                {
                    "textDocument": {
                        "uri": uri,
                        "languageId": "Cangjie",
                        "version": 0,
                        "text": text,
                    },
                },
            )

    async def wait_for_diagnostics(self, file_path: str, timeout: float = 3.0) -> list[Diagnostic]:
        """Wait for diagnostics for a file.

        Args:
            file_path: Absolute path to the file
            timeout: Maximum wait time in seconds

        Returns:
            List of diagnostics
        """
        start = asyncio.get_event_loop().time()
        while asyncio.get_event_loop().time() - start < timeout:
            if file_path in self._diagnostics:
                return self._diagnostics[file_path]
            await asyncio.sleep(0.1)
        return self._diagnostics.get(file_path, [])

    # =========================================================================
    # LSP Operations
    # =========================================================================

    async def definition(self, file_path: str, line: int, character: int) -> list[Location]:
        """Get definition locations.

        Args:
            file_path: Absolute path to the file
            line: Line number (0-based)
            character: Character position (0-based)

        Returns:
            List of Location models
        """
        await self.open_file(file_path)
        uri = Path(file_path).as_uri()

        result = await self._send_request(
            "textDocument/definition",
            {
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
            },
        )

        return self._parse_locations(result)

    async def references(self, file_path: str, line: int, character: int) -> list[Location]:
        """Find all references.

        Args:
            file_path: Absolute path to the file
            line: Line number (0-based)
            character: Character position (0-based)

        Returns:
            List of Location models
        """
        await self.open_file(file_path)
        uri = Path(file_path).as_uri()

        result = await self._send_request(
            "textDocument/references",
            {
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
                "context": {"includeDeclaration": True},
            },
        )

        return self._parse_locations(result or [])

    async def hover(self, file_path: str, line: int, character: int) -> HoverResult | None:
        """Get hover information.

        Args:
            file_path: Absolute path to the file
            line: Line number (0-based)
            character: Character position (0-based)

        Returns:
            HoverResult or None
        """
        await self.open_file(file_path)
        uri = Path(file_path).as_uri()

        result = await self._send_request(
            "textDocument/hover",
            {
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
            },
        )

        if result is None:
            return None
        return HoverResult.model_validate(result)

    async def document_symbol(self, file_path: str) -> list[DocumentSymbol]:
        """Get document symbols.

        Args:
            file_path: Absolute path to the file

        Returns:
            List of DocumentSymbol models
        """
        await self.open_file(file_path)
        uri = Path(file_path).as_uri()

        result = await self._send_request(
            "textDocument/documentSymbol",
            {"textDocument": {"uri": uri}},
        )

        if not result:
            return []
        return [DocumentSymbol.model_validate(sym) for sym in result]

    async def completion(self, file_path: str, line: int, character: int) -> list[CompletionItem]:
        """Get code completion.

        Args:
            file_path: Absolute path to the file
            line: Line number (0-based)
            character: Character position (0-based)

        Returns:
            List of CompletionItem models
        """
        await self.open_file(file_path)
        uri = Path(file_path).as_uri()

        result = await self._send_request(
            "textDocument/completion",
            {
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
            },
        )

        if result is None:
            return []
        if is_dict(result) and "items" in result:
            items = result["items"]
        elif is_list(result):
            items = result
        else:
            return []
        return [CompletionItem.model_validate(item) for item in items]

    async def get_diagnostics(self, file_path: str) -> list[Diagnostic]:
        """Get cached diagnostics for a file.

        Args:
            file_path: Absolute path to the file

        Returns:
            List of Diagnostic models
        """
        await self.open_file(file_path)
        return await self.wait_for_diagnostics(file_path)

    def _parse_locations(self, result: Any) -> list[Location]:  # noqa: ANN401
        """Parse LSP location result into typed models.

        Args:
            result: LSP location result (single or list)

        Returns:
            List of Location models
        """
        if result is None:
            return []
        if is_dict(result):
            return [Location.model_validate(result)]
        if is_list(result):
            return [Location.model_validate(loc) for loc in result if loc is not None]
        return []

    # =========================================================================
    # Lifecycle
    # =========================================================================

    async def shutdown(self) -> None:
        """Shutdown the LSP server."""
        if self._message_task:
            self._message_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._message_task

        if self._initialized:
            try:
                await asyncio.wait_for(
                    self._send_request("shutdown", {}),
                    timeout=5.0,
                )
                await self._send_notification("exit", {})
            except Exception as e:
                logger.warning(f"Error during LSP shutdown: {e}")

        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
            self.process = None

        self._initialized = False
        logger.info("LSP client shutdown complete")
