#!/usr/bin/env python3
"""Package native Cargo builds and validate the complete release asset set."""

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import urllib.request
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux", "x64"),
    "aarch64-apple-darwin": ("macos", "arm64"),
    "x86_64-apple-darwin": ("macos", "x64"),
    "x86_64-pc-windows-msvc": ("windows", "x64"),
}
FORMATS = {
    "linux": (".tar.gz", ".deb", ".rpm", ".pkg.tar.zst"),
    "macos": (".zip", ".dmg"),
    "windows": (".zip", ".exe"),
}


def version():
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    for filename in ("package.json", "tauri.conf.json"):
        if json.loads((ROOT / filename).read_text())["version"] != cargo:
            raise RuntimeError(f"{filename} version must match Cargo.toml ({cargo})")
    return cargo


def artifact_name(target, release_version, extension, configuration="Release"):
    platform, arch = TARGETS[target]
    suffix = "-debug" if configuration == "Debug" else ""
    return f"tabius-{platform}-{arch}-{release_version}{suffix}{extension}"


def checksum(path):
    with path.open("rb") as source:
        return f"{hashlib.file_digest(source, 'sha256').hexdigest()}  {path.name}\n"


def smoke(executable, release_version):
    environment = os.environ.copy()
    environment.pop("LD_LIBRARY_PATH", None)
    result = subprocess.run(
        [str(executable), "--version"],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
        env=environment,
        cwd=executable.parent,
    )
    if result.stdout.strip() != f"tabius-desktop {release_version}":
        raise RuntimeError(f"Unexpected executable version: {result.stdout!r}")
    subprocess.run(
        [str(executable), "--help"],
        check=True,
        capture_output=True,
        timeout=30,
        env=environment,
        cwd=executable.parent,
    )


def linux_packages(executable, output, target, release_version, stage):
    root = stage / "root"
    binary = root / "usr/bin/tabius-desktop"
    binary.parent.mkdir(parents=True)
    shutil.copy2(executable, binary)
    subprocess.run(
        ["desktop-file-validate", "packaging/linux/dev.tabius.desktop.desktop"],
        cwd=ROOT,
        check=True,
    )
    environment = dict(os.environ, PACKAGE_ROOT=str(root), PACKAGE_VERSION=release_version)
    packages = []
    for packager, extension in zip(("deb", "rpm", "archlinux"), FORMATS["linux"][1:], strict=True):
        package = output / artifact_name(target, release_version, extension)
        subprocess.run(
            [
                "nfpm",
                "package",
                "--config",
                "packaging/nfpm.yml",
                "--packager",
                packager,
                "--target",
                str(package),
            ],
            cwd=ROOT,
            env=environment,
            check=True,
        )
        packages.append(package)
    return packages


def native_bundles(target, configuration, installers, build):
    platform, _ = TARGETS[target]
    command = ["bun", "run", "tauri", "bundle", "--ci", "--target", target, "--bundles"]
    command.append(
        "app,dmg"
        if platform == "macos" and installers
        else "app"
        if platform == "macos"
        else "nsis"
    )
    if configuration == "Debug":
        command.append("--debug")
    subprocess.run(command, cwd=ROOT, check=True)
    if platform == "macos":
        bundle = build / "bundle/macos/Tabius.app"
        if not bundle.is_dir():
            raise RuntimeError(f"Missing macOS application bundle: {bundle}")
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(bundle)], check=True)
        return bundle
    return None


def package(args):
    target = args.target
    platform, _ = TARGETS[target]
    if {"linux": "linux", "darwin": "macos", "win32": "windows"}.get(sys.platform) != platform:
        raise RuntimeError("Packaging must run natively on the target OS")
    if args.installers and args.configuration != "Release":
        raise RuntimeError("Installers require a Release build")
    release_version = version()
    profile = "debug" if args.configuration == "Debug" else "release"
    build = (args.build_dir or ROOT / "target" / target / profile).resolve()
    executable = build / ("tabius-desktop.exe" if platform == "windows" else "tabius-desktop")
    if not executable.is_file():
        raise RuntimeError(f"Build the application first: missing {executable}")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    bundle = None
    if platform == "macos" or (platform == "windows" and args.installers):
        bundle = native_bundles(target, args.configuration, args.installers, build)
    extension = FORMATS[platform][0]
    archive = output / artifact_name(target, release_version, extension, args.configuration)
    artifacts = [archive]
    with tempfile.TemporaryDirectory(prefix="tabius-package-") as temporary:
        stage = pathlib.Path(temporary)
        content = stage / archive.name.removesuffix(extension)
        content.mkdir()
        shutil.copy2(ROOT / "LICENSE", content / "LICENSE")
        shutil.copy2(ROOT / "README.md", content / "README.md")
        if bundle:
            shutil.copytree(bundle, content / bundle.name)
        else:
            shutil.copy2(executable, content / executable.name)
        if platform == "linux":
            with tarfile.open(archive, "w:gz") as destination:
                destination.add(content, arcname=content.name)
        else:
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as destination:
                for path in sorted(content.rglob("*")):
                    destination.write(path, path.relative_to(stage))
        if args.installers:
            if platform == "linux":
                artifacts.extend(linux_packages(executable, output, target, release_version, stage))
            else:
                pattern = "bundle/dmg/*.dmg" if platform == "macos" else "bundle/nsis/*.exe"
                candidates = list(build.glob(pattern))
                if len(candidates) != 1:
                    raise RuntimeError(f"Expected exactly one installer under {build / pattern}")
                installer = output / artifact_name(target, release_version, FORMATS[platform][1])
                shutil.copy2(candidates[0], installer)
                artifacts.append(installer)
                if args.verify and platform == "macos":
                    subprocess.run(["hdiutil", "verify", str(installer)], check=True)
        if args.verify:
            verify_archive(archive, platform, release_version)
    for artifact in artifacts:
        if artifact.stat().st_size == 0:
            raise RuntimeError(f"Empty artifact: {artifact}")
        artifact.with_name(artifact.name + ".sha256").write_text(
            checksum(artifact),
            encoding="utf-8",
            newline="\n",
        )
        print(artifact)


def verify_archive(archive, platform, release_version):
    with tempfile.TemporaryDirectory(prefix="tabius-archive-check-") as temporary:
        extracted = pathlib.Path(temporary)
        if platform == "linux":
            with tarfile.open(archive, "r:gz") as source:
                source.extractall(extracted, filter="data")
        else:
            with zipfile.ZipFile(archive) as source:
                source.extractall(extracted)
                if os.name != "nt":
                    for member in source.infolist():
                        mode = (member.external_attr >> 16) & 0o777
                        if mode:
                            (extracted / member.filename).chmod(mode)
        executable_name = "tabius-desktop.exe" if platform == "windows" else "tabius-desktop"
        candidates = list(extracted.rglob(executable_name))
        if len(candidates) != 1:
            raise RuntimeError("Archive must contain exactly one application executable")
        smoke(candidates[0], release_version)
        if platform == "macos":
            bundle = candidates[0].parents[2]
            subprocess.run(["codesign", "--verify", "--deep", "--strict", str(bundle)], check=True)


def verify_release(directory, tag):
    release_version = version()
    if tag and tag.removeprefix("v") != release_version:
        raise RuntimeError(f"Release tag {tag!r} must match version {release_version}")
    expected = {
        artifact_name(target, release_version, extension)
        for target, (platform, _) in TARGETS.items()
        for extension in FORMATS[platform]
    }
    actual = {p.name for p in directory.iterdir() if p.name != "checksums.txt"}
    if actual != expected | {name + ".sha256" for name in expected}:
        raise RuntimeError("Release must contain all ten native assets and their checksum files")
    lines = []
    for name in sorted(expected):
        artifact = directory / name
        if artifact.stat().st_size == 0:
            raise RuntimeError(f"Empty artifact: {name}")
        line = checksum(artifact)
        if artifact.with_name(name + ".sha256").read_text() != line:
            raise RuntimeError(f"Checksum mismatch: {name}")
        lines.append(line)
    (directory / "checksums.txt").write_text("".join(lines), encoding="utf-8", newline="\n")


def install_tools():
    tool_version = json.loads((ROOT / "packaging/tools.json").read_text())["nfpm"]
    filename = f"nfpm_{tool_version}_Linux_x86_64.tar.gz"
    base = f"https://github.com/goreleaser/nfpm/releases/download/v{tool_version}"
    tool_root = pathlib.Path(os.environ.get("RUNNER_TEMP", tempfile.gettempdir()))
    destination = tool_root / "tabius-tools"
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="tabius-nfpm-") as temporary:
        archive = pathlib.Path(temporary) / filename
        with urllib.request.urlopen(f"{base}/checksums.txt", timeout=60) as response:
            lines = response.read().decode().splitlines()
        digests = [line.split()[0] for line in lines if line.split()[-1].lstrip("*") == filename]
        if len(digests) != 1:
            raise RuntimeError("Expected one checksum for the pinned nFPM archive")
        with urllib.request.urlopen(f"{base}/{filename}", timeout=120) as response:
            archive.write_bytes(response.read())
        with archive.open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != digests[0]:
                raise RuntimeError("nFPM download checksum mismatch")
        with tarfile.open(archive, "r:gz") as source:
            member = source.getmember("nfpm")
            if not member.isfile():
                raise RuntimeError("Expected a regular nFPM executable")
            with source.extractfile(member) as binary:
                (destination / "nfpm").write_bytes(binary.read())
        (destination / "nfpm").chmod(0o755)
    subprocess.run([str(destination / "nfpm"), "--version"], check=True)
    if path_file := os.environ.get("GITHUB_PATH"):
        with open(path_file, "a", encoding="utf-8") as output:
            output.write(f"{destination}\n")
    else:
        print(f"Add {destination} to PATH before packaging")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("package")
    build.add_argument("--configuration", choices=("Debug", "Release"), default="Release")
    build.add_argument("--target", choices=TARGETS, required=True)
    build.add_argument("--build-dir", type=pathlib.Path)
    build.add_argument("--output-dir", type=pathlib.Path, default=ROOT / "artifacts")
    build.add_argument("--verify", action="store_true")
    build.add_argument("--installers", action="store_true")
    commands.add_parser("install-tools")
    verify = commands.add_parser("verify")
    verify.add_argument("--artifacts", type=pathlib.Path, default=ROOT / "artifacts")
    verify.add_argument("--tag")
    verify.add_argument("--tag-only", action="store_true")
    args = parser.parse_args()
    if args.command == "package":
        package(args)
    elif args.command == "install-tools":
        install_tools()
    elif args.tag_only:
        if args.tag is None or args.tag.removeprefix("v") != version():
            verify.error(f"Expected release tag v{version()} or {version()}")
    else:
        verify_release(args.artifacts, args.tag)


if __name__ == "__main__":
    main()
