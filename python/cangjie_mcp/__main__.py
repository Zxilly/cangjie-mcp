from __future__ import annotations

import os
import sys

from cangjie_mcp import find_cangjie_mcp_bin


def _run() -> None:
    cangjie_mcp = find_cangjie_mcp_bin()

    if sys.platform == "win32":
        import subprocess

        # Avoid emitting a traceback on interrupt
        try:
            completed_process = subprocess.run([cangjie_mcp, *sys.argv[1:]])
        except KeyboardInterrupt:
            sys.exit(2)

        sys.exit(completed_process.returncode)
    else:
        os.execvp(cangjie_mcp, [cangjie_mcp, *sys.argv[1:]])


if __name__ == "__main__":
    _run()
