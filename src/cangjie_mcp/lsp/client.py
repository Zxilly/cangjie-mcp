"""LSP client implementation using sansio-lsp-client.

This module wraps sansio-lsp-client (Sans-I/O) with asyncio subprocess I/O
to communicate with the Cangjie Language Server.
"""

from __future__ import annotations

import asyncio
import contextlib
import logging
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from sansio_lsp_client.client import CAPABILITIES, ClientState
from sansio_lsp_client.client import Client as SansioClient
from sansio_lsp_client.events import (
    Completion,
    ConfigurationRequest,
    Definition,
    Event,
    Hover,
    Initialized,
    LogMessage,
    MDocumentSymbols,
    PublishDiagnostics,
    References,
    RegisterCapabilityRequest,
    ResponseError,
    WorkDoneProgressCreate,
)
from sansio_lsp_client.structs import (
    Diagnostic,
    Id,
    Position,
    Request,
    TextDocumentContentChangeEvent,
    TextDocumentIdentifier,
    TextDocumentItem,
    TextDocumentPosition,
    VersionedTextDocumentIdentifier,
    WorkspaceFolder,
)

from cangjie_mcp.lsp.config import LSPInitOptions, LSPSettings

logger = logging.getLogger(__name__)


class _CangjieProtocolClient(SansioClient):
    """Subclass that injects initializationOptions and rootPath into initialize."""

    def __init__(  # noqa: ANN204
        self,
        *,
        process_id: int | None = None,
        root_uri: str | None = None,
        root_path: str | None = None,
        workspace_folders: list[WorkspaceFolder] | None = None,
        initialization_options: dict[str, Any] | None = None,
        trace: str = "off",
    ):
        # Bypass SansioClient.__init__ to inject custom params
        self._state = ClientState.NOT_INITIALIZED
        self._recv_buf = bytearray()
        self._send_buf = bytearray()
        self._unanswered_requests: dict[Id, Request] = {}
        self._id_counter = 0

        self._send_request(
            method="initialize",
            params={
                "processId": process_id,
                "rootUri": root_uri,
                "rootPath": root_path,
                "workspaceFolders": (
                    [f.model_dump() for f in workspace_folders] if workspace_folders is not None else None
                ),
                "trace": trace,
                "capabilities": CAPABILITIES,
                "initializationOptions": initialization_options or {},
            },
        )
        self._state = ClientState.WAITING_FOR_INITIALIZED


@dataclass
class CangjieClient:
    """Cangjie LSP client.

    Wraps sansio-lsp-client for protocol handling and manages the LSP
    server subprocess via asyncio.
    """

    settings: LSPSettings
    init_options: LSPInitOptions
    env: dict[str, str]

    _process: asyncio.subprocess.Process | None = field(default=None, init=False)
    _client: _CangjieProtocolClient | None = field(default=None, init=False)
    _pending: dict[Id, asyncio.Future[Event]] = field(
        default_factory=lambda: dict[Id, asyncio.Future[Event]](),
        init=False,
    )
    _diagnostics: dict[str, list[Diagnostic]] = field(
        default_factory=lambda: dict[str, list[Diagnostic]](),
        init=False,
    )
    _open_files: dict[str, int] = field(default_factory=lambda: dict[str, int](), init=False)
    _initialized_event: asyncio.Event = field(default_factory=asyncio.Event, init=False)
    _initialized: bool = field(default=False, init=False)
    _init_error: str | None = field(default=None, init=False)
    _read_task: asyncio.Task[None] | None = field(default=None, init=False)
    _stderr_task: asyncio.Task[None] | None = field(default=None, init=False)
    _stderr_lines: list[str] = field(default_factory=lambda: list[str](), init=False)

    @property
    def is_initialized(self) -> bool:
        """Check if the client has been initialized."""
        return self._initialized

    @property
    def is_alive(self) -> bool:
        """Check if the LSP server process is still running."""
        return self._process is not None and self._process.returncode is None

    async def start(self, timeout: int = 45000) -> None:
        """Start the LSP server and initialize the connection.

        Args:
            timeout: Initialization timeout in milliseconds
        """
        settings = self.settings
        exe = "LSPServer.exe" if sys.platform == "win32" else "LSPServer"
        cmd = [str(settings.sdk_path / "tools" / "bin" / exe), *settings.get_lsp_args()]

        logger.info("Starting LSP server: %s", " ".join(cmd))

        self._process = await asyncio.create_subprocess_exec(
            *cmd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=self.env,
            cwd=str(settings.workspace_path),
        )

        root_uri = settings.workspace_path.as_uri()
        self._client = _CangjieProtocolClient(
            process_id=self._process.pid,
            root_uri=root_uri,
            root_path=str(settings.workspace_path),
            workspace_folders=[WorkspaceFolder(uri=root_uri, name="workspace")],
            initialization_options=self.init_options.model_dump(by_alias=True),
        )

        # Flush the initialize request
        await self._flush()

        # Start background read tasks
        self._read_task = asyncio.create_task(self._read_loop())
        if self._process.stderr:
            self._stderr_task = asyncio.create_task(self._read_stderr(self._process.stderr))

        # Wait for initialized (or early process exit)
        try:
            await asyncio.wait_for(self._initialized_event.wait(), timeout=timeout / 1000)
        except TimeoutError:
            alive = self.is_alive
            rc = self._process.returncode if self._process else None
            stderr_tail = self._stderr_lines[-20:]
            parts = [
                f"LSP server did not initialize within {timeout}ms",
                f"process alive: {alive}, exit code: {rc}",
            ]
            if stderr_tail:
                parts.append("stderr:\n" + "\n".join(stderr_tail))
            raise TimeoutError("\n".join(parts)) from None
        if self._init_error:
            raise RuntimeError(self._init_error)

    async def _flush(self) -> None:
        """Flush the sansio client send buffer to subprocess stdin."""
        if self._client is None or self._process is None or self._process.stdin is None:
            return
        data = self._client.send()
        if data:
            self._process.stdin.write(data)
            await self._process.stdin.drain()

    async def _read_loop(self) -> None:
        """Read from subprocess stdout and feed to sansio client."""
        assert self._process is not None and self._process.stdout is not None
        assert self._client is not None
        try:
            while not self._process.stdout.at_eof():
                data = await self._process.stdout.read(4096)
                if not data:
                    break
                for event in self._client.recv(data):
                    await self._handle_event(event)
        except asyncio.CancelledError:
            pass
        except Exception as e:
            logger.error("Error in LSP read loop: %s", e)
        finally:
            self._fail_pending()
            # If process exited before initialization, signal immediately
            # so start() doesn't wait for the full timeout.
            if not self._initialized:
                rc = self._process.returncode if self._process else None
                parts = [f"LSP server exited before initialization (exit code: {rc})"]
                # Brief wait for stderr task to flush remaining output
                if self._stderr_task and not self._stderr_task.done():
                    await asyncio.sleep(0.1)
                if self._stderr_lines:
                    parts.append("stderr:\n" + "\n".join(self._stderr_lines[-20:]))
                self._init_error = "\n".join(parts)
                self._initialized_event.set()

    def _fail_pending(self) -> None:
        """Fail all pending futures when the read loop exits."""
        pending = self._pending
        self._pending = {}
        for future in pending.values():
            if not future.done():
                future.set_exception(ConnectionError("LSP server disconnected"))

    async def _read_stderr(self, stderr: asyncio.StreamReader) -> None:
        """Read LSP server stderr, log it, and buffer for diagnostics."""
        while not stderr.at_eof():
            try:
                line = await stderr.readline()
                if line:
                    text = line.decode("utf-8", errors="replace").rstrip()
                    if text:
                        logger.debug("[LSP stderr] %s", text)
                        self._stderr_lines.append(text)
            except asyncio.CancelledError:
                break
            except Exception:
                break

    async def _handle_event(self, event: Event) -> None:
        """Dispatch a sansio-lsp-client Event."""
        if isinstance(event, Initialized):
            self._initialized = True
            self._initialized_event.set()
            logger.info("LSP client initialized (capabilities: %s)", list(event.capabilities.keys()))

        elif isinstance(event, PublishDiagnostics):
            path = _uri_to_path(event.uri)
            self._diagnostics[path] = list(event.diagnostics)
            logger.debug("Received %d diagnostics for %s", len(event.diagnostics), path)

        elif isinstance(event, WorkDoneProgressCreate):
            event.reply()
            await self._flush()

        elif isinstance(event, ConfigurationRequest):
            event.reply(result=[{}] * len(event.items))
            await self._flush()

        elif isinstance(event, RegisterCapabilityRequest):
            event.reply()
            await self._flush()

        elif isinstance(event, LogMessage):
            log_level = {1: logging.ERROR, 2: logging.WARNING, 3: logging.INFO, 4: logging.DEBUG, 5: logging.DEBUG}
            logger.log(log_level.get(event.type.value, logging.DEBUG), "[LSP server] %s", event.message)

        elif isinstance(event, ResponseError):
            msg_id = event.message_id
            if msg_id is not None and msg_id in self._pending:
                future = self._pending.pop(msg_id)
                if not future.done():
                    future.set_exception(Exception(f"LSP Error {event.code}: {event.message}"))
            else:
                logger.error("LSP error (no pending request): code=%s %s", event.code, event.message)

        else:
            # Response events — resolve pending future by message_id
            msg_id = getattr(event, "message_id", None)
            if msg_id is not None and msg_id in self._pending:
                future = self._pending.pop(msg_id)
                if not future.done():
                    future.set_result(event)

    async def _request(self, send_fn: Any, *args: Any) -> Event:  # noqa: ANN401
        """Send an LSP request and await the response event.

        Args:
            send_fn: Sansio client method that returns a message Id
            *args: Arguments to pass to send_fn

        Returns:
            The response Event from the server
        """
        msg_id: Id = send_fn(*args)
        future: asyncio.Future[Event] = asyncio.get_running_loop().create_future()
        self._pending[msg_id] = future
        await self._flush()
        return await future

    async def _ensure_file_open(self, file_path: str) -> None:
        """Ensure a file is opened in the LSP server.

        On first open, sends textDocument/didOpen.
        On subsequent calls, sends textDocument/didChange with full content.

        Args:
            file_path: Absolute path to the file
        """
        assert self._client is not None
        path = Path(file_path)
        if not path.exists():
            raise FileNotFoundError(f"File not found: {file_path}")

        text = path.read_text(encoding="utf-8")
        uri = path.as_uri()

        if file_path in self._open_files:
            version = self._open_files[file_path] + 1
            self._open_files[file_path] = version
            self._client.did_change(
                VersionedTextDocumentIdentifier(uri=uri, version=version),
                [TextDocumentContentChangeEvent.whole_document_change(text)],
            )
        else:
            self._open_files[file_path] = 0
            self._client.did_open(
                TextDocumentItem(uri=uri, languageId="Cangjie", version=0, text=text),
            )

        await self._flush()

    def _make_tdp(self, file_path: str, line: int, character: int) -> TextDocumentPosition:
        """Create a TextDocumentPosition."""
        return TextDocumentPosition(
            textDocument=TextDocumentIdentifier(uri=Path(file_path).as_uri()),
            position=Position(line=line, character=character),
        )

    # =========================================================================
    # LSP Operations
    # =========================================================================

    async def definition(self, file_path: str, line: int, character: int) -> Definition:
        """Get definition locations.

        Args:
            file_path: Absolute path to the file
            line: Line number (0-based)
            character: Character position (0-based)

        Returns:
            Definition event with result locations
        """
        await self._ensure_file_open(file_path)
        assert self._client is not None
        event = await self._request(self._client.definition, self._make_tdp(file_path, line, character))
        assert isinstance(event, Definition)
        return event

    async def references(self, file_path: str, line: int, character: int) -> References:
        """Find all references.

        Args:
            file_path: Absolute path to the file
            line: Line number (0-based)
            character: Character position (0-based)

        Returns:
            References event with result locations
        """
        await self._ensure_file_open(file_path)
        assert self._client is not None
        event = await self._request(self._client.references, self._make_tdp(file_path, line, character))
        assert isinstance(event, References)
        return event

    async def hover(self, file_path: str, line: int, character: int) -> Hover:
        """Get hover information.

        Args:
            file_path: Absolute path to the file
            line: Line number (0-based)
            character: Character position (0-based)

        Returns:
            Hover event with contents
        """
        await self._ensure_file_open(file_path)
        assert self._client is not None
        event = await self._request(self._client.hover, self._make_tdp(file_path, line, character))
        assert isinstance(event, Hover)
        return event

    async def completion(self, file_path: str, line: int, character: int) -> Completion:
        """Get code completion.

        Args:
            file_path: Absolute path to the file
            line: Line number (0-based)
            character: Character position (0-based)

        Returns:
            Completion event with completion_list
        """
        await self._ensure_file_open(file_path)
        assert self._client is not None
        event = await self._request(self._client.completion, self._make_tdp(file_path, line, character))
        assert isinstance(event, Completion)
        return event

    async def document_symbol(self, file_path: str) -> MDocumentSymbols:
        """Get document symbols.

        Args:
            file_path: Absolute path to the file

        Returns:
            MDocumentSymbols event with result symbols
        """
        await self._ensure_file_open(file_path)
        assert self._client is not None
        td = TextDocumentIdentifier(uri=Path(file_path).as_uri())
        event = await self._request(self._client.documentSymbol, td)
        assert isinstance(event, MDocumentSymbols)
        return event

    async def get_diagnostics(self, file_path: str, timeout: float = 3.0) -> list[Diagnostic]:
        """Get diagnostics for a file (from push cache).

        Args:
            file_path: Absolute path to the file
            timeout: Maximum wait time in seconds

        Returns:
            List of Diagnostic models
        """
        await self._ensure_file_open(file_path)
        start = asyncio.get_event_loop().time()
        while asyncio.get_event_loop().time() - start < timeout:
            if file_path in self._diagnostics:
                return self._diagnostics[file_path]
            await asyncio.sleep(0.1)
        return self._diagnostics.get(file_path, [])

    # =========================================================================
    # Lifecycle
    # =========================================================================

    async def shutdown(self) -> None:
        """Shutdown the LSP server."""
        # Cancel stderr task first (just logging)
        if self._stderr_task:
            self._stderr_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._stderr_task

        # Send shutdown/exit while read loop is still running
        if self._initialized and self._client is not None:
            try:
                self._client.shutdown()
                await self._flush()
                # Brief wait for the server to process shutdown
                await asyncio.sleep(0.5)
                # Force state for exit (we may not have received the response)
                if getattr(self._client, "_state", None) == ClientState.WAITING_FOR_SHUTDOWN:
                    object.__setattr__(self._client, "_state", ClientState.SHUTDOWN)
                self._client.exit()
                await self._flush()
            except Exception as e:
                logger.warning("Error during LSP shutdown: %s", e)

        # Now cancel read task
        if self._read_task:
            self._read_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._read_task

        # Terminate process
        if self._process:
            self._process.terminate()
            try:
                await asyncio.wait_for(self._process.wait(), timeout=5)
            except TimeoutError:
                self._process.kill()
            self._process = None

        self._initialized = False
        logger.info("LSP client shutdown complete")


def _uri_to_path(uri: str) -> str:
    """Convert a file URI to a filesystem path."""
    from urllib.parse import unquote, urlparse

    path = unquote(urlparse(uri).path)
    # Windows: remove leading slash from /C:/path
    if sys.platform == "win32" and path.startswith("/") and len(path) > 2 and path[2] == ":":
        path = path[1:]
    return path
