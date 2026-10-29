"""Writes the archive fixtures of rc-vfs's archive backend (RC-030-01).

The fixtures are committed; run this only to change them, from this directory:

    uv run --no-project --with py7zr==1.1.4 --with pyzipper==0.3.6 --with zstandard==0.25.0 \
        python generate.py

Tools from other implementations than the Rust crates that read them, so a test proves the
reader against what other programs write. `zip` (Info-ZIP) must be on PATH for the ZipCrypto
fixture. Every timestamp is MTIME, so the tests can compare times exactly.
"""

import bz2
import gzip
import io
import lzma
import os
import struct
import subprocess
import tarfile
import tempfile
import time
import zipfile
import zlib

import py7zr
import pyzipper
import zstandard

HERE = os.path.dirname(os.path.abspath(__file__))
MTIME = 1715949296  # 2024-05-17 12:34:56 UTC

# The tree every multi-entry format carries; `None` is a directory. `deep/` and `deep/nested/`
# have no entry of their own in zip and tar: the reader must make them up.
TREE = [
    ("hello.txt", b"hello archive\n"),
    ("docs/", None),
    ("docs/readme.md", b"# Readme\n\nInside an archive.\n"),
    ("deep/nested/data.bin", bytes(i % 251 for i in range(3000))),
    ("empty.txt", b""),
    ("unicode/Gr\u00fc\u00dfe.txt", b"utf8\n"),
    ("long/" + "n" * 120 + ".txt", b"long\n"),
    ("conformance/", None),
    ("conformance/a.txt", b"hello"),
    ("conformance/b.bin", bytes(1024)),
    ("conformance/sub/", None),
]

PAYLOAD = b"single file payload\n" * 100
LARGE = 32 * 1024 * 1024


def out(name):
    return os.path.join(HERE, name)


def ut_field(mtime):
    """The Info-ZIP extended timestamp: the modification time in UTC."""
    return struct.pack("<HHBl", 0x5455, 5, 1, mtime)


def zip_info(name, is_dir):
    info = zipfile.ZipInfo(name, time.gmtime(MTIME)[:6])
    info.create_system = 3
    info.extra = ut_field(MTIME)
    if is_dir:
        info.external_attr = (0o40755 << 16) | 0x10
        info.compress_type = zipfile.ZIP_STORED
    else:
        info.external_attr = 0o100644 << 16
        info.compress_type = zipfile.ZIP_DEFLATED
    return info


def write_zip(path, entries):
    with zipfile.ZipFile(path, "w") as archive:
        for name, data in entries:
            archive.writestr(zip_info(name, data is None), data or b"")


def tar_bytes(entries):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.PAX_FORMAT) as archive:
        for name, data in entries:
            info = tarfile.TarInfo(name.rstrip("/") if data is None else name)
            info.mtime = MTIME
            if data is None:
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                archive.addfile(info)
            else:
                info.mode = 0o644
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def write_tree_on_disk(root, entries):
    for name, data in entries:
        path = os.path.join(root, *name.rstrip("/").split("/"))
        if data is None:
            os.makedirs(path, exist_ok=True)
        else:
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "wb") as file:
                file.write(data)
    for directory, dirs, files in os.walk(root, topdown=False):
        for name in files + dirs:
            os.utime(os.path.join(directory, name), (MTIME, MTIME))


def seven_zip(path, entries, **options):
    with tempfile.TemporaryDirectory() as staging:
        write_tree_on_disk(staging, entries)
        with py7zr.SevenZipFile(path, "w", **options) as archive:
            for name in sorted(os.listdir(staging)):
                archive.writeall(os.path.join(staging, name), name)


def patched_zip(path, placeholder, encoded):
    """A zip whose one name is stored in a legacy code page, without the UTF-8 flag."""
    assert len(placeholder) == len(encoded)
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        archive.writestr(zip_info(placeholder.decode("ascii"), False), b"legacy name\n")
    data = buffer.getvalue()
    assert data.count(placeholder) == 2
    with open(path, "wb") as file:
        file.write(data.replace(placeholder, encoded))


def tree_formats():
    write_zip(out("tree.zip"), TREE)
    raw = tar_bytes(TREE)
    with open(out("tree.tar"), "wb") as file:
        file.write(raw)
    with open(out("tree.tar.gz"), "wb") as file:
        file.write(gzip.compress(raw, mtime=0))
    with open(out("tree.tar.xz"), "wb") as file:
        file.write(lzma.compress(raw, format=lzma.FORMAT_XZ))
    with open(out("tree.tar.bz2"), "wb") as file:
        file.write(bz2.compress(raw))
    with open(out("tree.tar.zst"), "wb") as file:
        file.write(zstandard.ZstdCompressor(write_content_size=True).compress(raw))
    seven_zip(out("tree.7z"), TREE)


def single_files():
    with open(out("single.txt.gz"), "wb") as file:
        file.write(gzip.compress(PAYLOAD, mtime=0))
    with open(out("single.txt.xz"), "wb") as file:
        file.write(lzma.compress(PAYLOAD, format=lzma.FORMAT_XZ))
    with open(out("single.txt.bz2"), "wb") as file:
        file.write(bz2.compress(PAYLOAD))
    with open(out("single.txt.zst"), "wb") as file:
        file.write(zstandard.ZstdCompressor(write_content_size=True).compress(PAYLOAD))
    # Two frames: the reader must not stop after the first.
    half = len(PAYLOAD) // 2
    frames = zstandard.ZstdCompressor().compress(PAYLOAD[:half])
    frames += zstandard.ZstdCompressor().compress(PAYLOAD[half:])
    with open(out("frames.txt.zst"), "wb") as file:
        file.write(frames)


def safety():
    write_zip(
        out("unsafe.zip"),
        [
            ("safe.txt", b"safe\n"),
            ("../evil.txt", b"evil\n"),
            ("/abs/root.txt", b"absolute\n"),
            ("C:/drive.txt", b"drive\n"),
            ("D:\\back\\slash.txt", b"backslash drive\n"),
            ("\\\\server\\share\\unc.txt", b"unc\n"),
            ("ok/../../escape.txt", b"escape\n"),
            ("../safe.txt", b"shadow\n"),
        ],
    )
    # A symbolic link member: the content is the target.
    with zipfile.ZipFile(out("unsafe.zip"), "a") as archive:
        info = zip_info("link", False)
        info.external_attr = 0o120777 << 16
        info.compress_type = zipfile.ZIP_STORED
        archive.writestr(info, b"/etc/passwd")

    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.PAX_FORMAT) as archive:
        for name, data in [("safe.txt", b"safe\n"), ("../evil.txt", b"evil\n"), ("/abs.txt", b"abs\n")]:
            info = tarfile.TarInfo(name)
            info.mtime, info.mode, info.size = MTIME, 0o644, len(data)
            archive.addfile(info, io.BytesIO(data))
        for name, kind, target in [
            ("link", tarfile.SYMTYPE, "../../etc/passwd"),
            ("abs-link", tarfile.SYMTYPE, "/etc/shadow"),
            ("hard", tarfile.LNKTYPE, "/etc/passwd"),
        ]:
            info = tarfile.TarInfo(name)
            info.mtime, info.mode, info.type, info.linkname = MTIME, 0o777, kind, target
            archive.addfile(info)
    with open(out("unsafe.tar"), "wb") as file:
        file.write(buffer.getvalue())


def encodings():
    patched_zip(out("cp437.zip"), b"GrXXe.txt", "Gr\u00f6\u00dfe.txt".encode("cp437"))
    russian = "\u041f\u0440\u0438\u0432\u0435\u0442.txt"
    patched_zip(out("cp866.zip"), b"XXXXXX.txt", russian.encode("cp866"))


def encrypted():
    with pyzipper.AESZipFile(
        out("encrypted-aes.zip"), "w", compression=pyzipper.ZIP_DEFLATED, encryption=pyzipper.WZ_AES
    ) as archive:
        archive.setpassword(b"secret")
        archive.writestr("secret.txt", b"aes secret\n")
    with tempfile.TemporaryDirectory() as staging:
        with open(os.path.join(staging, "secret.txt"), "wb") as file:
            file.write(b"zipcrypto secret\n")
        target = out("encrypted-zipcrypto.zip")
        if os.path.exists(target):
            os.remove(target)
        subprocess.run(["zip", "-q", "-X", "-P", "secret", target, "secret.txt"], cwd=staging, check=True)
    seven_zip(out("encrypted.7z"), [("secret.txt", b"7z secret\n")], password="secret")
    seven_zip(
        out("encrypted-header.7z"),
        [("secret.txt", b"7z secret\n")],
        password="secret",
        header_encryption=True,
    )


def limits():
    zeros = bytes(16 * 1024 * 1024)
    with zipfile.ZipFile(out("bomb.zip"), "w") as archive:
        info = zip_info("zeros.bin", False)
        archive.writestr(info, zeros, compresslevel=9)
    with open(out("bomb.tar.gz"), "wb") as file:
        file.write(gzip.compress(tar_bytes([("zeros.bin", zeros)]), compresslevel=9, mtime=0))


def large_solid():
    first = bytes(i % 251 for i in range(251)) * (LARGE // 251 + 1)
    second = bytes(i % 253 for i in range(253)) * (LARGE // 253 + 1)
    seven_zip(out("solid-large.7z"), [("first.bin", first[:LARGE]), ("second.bin", second[:LARGE])])


def zip64(name, data):
    """One stored member in the zip64 layout throughout: 0xFFFFFFFF sizes and offset with the
    real ones in zip64 extra fields, and the zip64 end records. `zipfile` writes these only
    past 4 GiB."""
    full = 0xFFFFFFFF
    crc = zlib.crc32(data)
    dos_time, dos_date = 0x645C, 0x58B1  # 12:34:56 2024-05-17
    encoded = name.encode("ascii")
    local_extra = struct.pack("<HHQQ", 1, 16, len(data), len(data)) + ut_field(MTIME)
    local = struct.pack(
        "<IHHHHHIIIHH", 0x04034B50, 45, 0, 0, dos_time, dos_date, crc, full, full,
        len(encoded), len(local_extra),
    ) + encoded + local_extra + data
    central_extra = struct.pack("<HHQQQ", 1, 24, len(data), len(data), 0) + ut_field(MTIME)
    central = struct.pack(
        "<IHHHHHHIIIHHHHHII", 0x02014B50, (3 << 8) | 45, 45, 0, 0, dos_time, dos_date, crc,
        full, full, len(encoded), len(central_extra), 0, 0, 0, 0o100644 << 16, full,
    ) + encoded + central_extra
    end64_offset = len(local) + len(central)
    end64 = struct.pack(
        "<IQHHIIQQQQ", 0x06064B50, 44, (3 << 8) | 45, 45, 0, 0, 1, 1, len(central), len(local)
    )
    locator = struct.pack("<IIQI", 0x07064B50, 0, end64_offset, 1)
    end = struct.pack("<IHHHHIIH", 0x06054B50, 0, 0, 0xFFFF, 0xFFFF, full, full, 0)
    with open(out(name.replace(".txt", ".zip")), "wb") as file:
        file.write(local + central + end64 + locator + end)
    with zipfile.ZipFile(out(name.replace(".txt", ".zip"))) as check:
        assert check.read(name) == data


def others():
    zip64("zip64.txt", b"zip64 entry\n")
    with open(out("not-a-zip.zip"), "wb") as file:
        file.write(b"this is not a zip archive\n" * 3)


def main():
    tree_formats()
    single_files()
    safety()
    encodings()
    encrypted()
    limits()
    large_solid()
    others()
    for name in sorted(os.listdir(HERE)):
        if name != os.path.basename(__file__):
            print(f"{os.path.getsize(out(name)):>8}  {name}")


if __name__ == "__main__":
    main()
