import argparse
import subprocess
import sys
import time
import os
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parent
TEST_DIR = PROJECT_ROOT / "dev" / "test"
BIN_DIR = TEST_DIR / "bin"
OBJECT_FILE = BIN_DIR / "out.o"
RUST_FILE = TEST_DIR / "test.rs"
EXE_FILE = BIN_DIR / "test.exe"
IR_EXPLORER_DIR = PROJECT_ROOT / "ir-explorer"


def run_command(command, cwd=None):
    print(f"$ {' '.join(map(str, command))}")

    result = subprocess.run(command, cwd=cwd)

    if result.returncode != 0:
        sys.exit(result.returncode)


def compile_project():
    BIN_DIR.mkdir(parents=True, exist_ok=True)

    run_command([
        "cargo",
        "run",
        "--release",
        "--",
        "-e",
        "ir",
        str(TEST_DIR),
        "-o",
        str(OBJECT_FILE),
    ])


def run_project():
    compile_project()

    run_command([
        "rustc",
        str(RUST_FILE),
        "-C", "opt-level=3",
        "-C", "overflow-checks=off",
        "-C", f"link-arg={OBJECT_FILE}",
        "-C", "lto=fat",
        "-C", "codegen-units=1",
        "-o", str(EXE_FILE),
    ])

    run_command([str(EXE_FILE)])

def explore():
    command = [
        "cargo",
        "run",
        "--release",
        "--",
        "../dev/test",
    ]

    print(f"$ {' '.join(command)}")

    kwargs = {
        "cwd": IR_EXPLORER_DIR,
        "stdin": subprocess.DEVNULL,
        "stdout": subprocess.DEVNULL,
        "stderr": subprocess.DEVNULL,
    }

    if sys.platform == "win32":
        kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW
    else:
        # Detach from the terminal/session that launched this script.
        kwargs["start_new_session"] = True

    subprocess.Popen(command, **kwargs)

def install_lsp():
    command = [
        "cargo",
        "install",
        "--path",
        "./lsp",
        "--force",
    ]

    print(f"$ {' '.join(command)}")

    while True:
        result = subprocess.run(command)

        if result.returncode == 0:
            break

        print("LSP is probably still running. Close the editor and retrying...")

        time.sleep(1)


def main():
    parser = argparse.ArgumentParser(
        description="Run project development pipelines."
    )

    subparsers = parser.add_subparsers(
        dest="command",
        required=True,
    )

    subparsers.add_parser(
        "compile",
        help="Compile the test project to an object file.",
    )

    subparsers.add_parser(
        "run",
        help="Compile, link, and run the test project.",
    )

    subparsers.add_parser(
        "explore",
        help="Run the IR explorer.",
    )

    subparsers.add_parser(
        "install-lsp",
        help="Install the LSP.",
    )

    args = parser.parse_args()

    if args.command == "compile":
        compile_project()
    elif args.command == "run":
        run_project()
    elif args.command == "explore":
        explore()
    elif args.command == "install-lsp":
        install_lsp()


if __name__ == "__main__":
    main()
