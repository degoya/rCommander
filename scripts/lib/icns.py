"""The macOS app icon, rcommander.icns, from the repository's PNG icons (RC-100-44).

An .icns file is a header ('icns', total length) and one entry per image (type, length, data);
since macOS 10.7 an entry may hold a PNG as it is, so no image tool is needed and the script
runs anywhere - the test included. The PNG images of each input are taken: a .png file itself,
an .ico file's PNG-encoded entries (its 256 px one). Each size lands in the types macOS reads it
from; a size without a type is left out, none at all is a failure.

    python3 -I scripts/lib/icns.py <out.icns> <icon.png|icon.ico>...
"""

import struct
import sys

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"

# The entry types of a PNG of this width: the size itself and the half size at 2x.
TYPES = {
    128: [b"ic07"],
    256: [b"ic08", b"ic13"],
    512: [b"ic09", b"ic14"],
    1024: [b"ic10"],
}


def png_size(data):
    """The (width, height) of a PNG, read from its IHDR chunk; None for anything else."""
    if not data.startswith(PNG_SIGNATURE) or data[12:16] != b"IHDR" or len(data) < 24:
        return None
    return struct.unpack(">II", data[16:24])


def ico_pngs(data):
    """The PNG-encoded images of an .ico file."""
    if len(data) < 6:
        return []
    _, kind, count = struct.unpack("<HHH", data[:6])
    if kind != 1:
        return []
    images = []
    for index in range(count):
        entry = data[6 + 16 * index : 22 + 16 * index]
        if len(entry) < 16:
            break
        size, offset = struct.unpack("<II", entry[8:16])
        image = data[offset : offset + size]
        if image.startswith(PNG_SIGNATURE):
            images.append(image)
    return images


def main(arguments):
    if len(arguments) < 2:
        print(__doc__.strip().splitlines()[-1].strip(), file=sys.stderr)
        return 2
    target, sources = arguments[0], arguments[1:]
    entries = {}
    for source in sources:
        with open(source, "rb") as handle:
            data = handle.read()
        images = ico_pngs(data) if source.lower().endswith(".ico") else [data]
        for image in images:
            size = png_size(image)
            if size is None or size[0] != size[1]:
                continue
            for kind in TYPES.get(size[0], []):
                entries.setdefault(kind, image)
    if not entries:
        print(f"!! no square PNG of {sorted(TYPES)} px in {' '.join(sources)}", file=sys.stderr)
        return 1
    body = b"".join(
        kind + struct.pack(">I", 8 + len(image)) + image for kind, image in sorted(entries.items())
    )
    with open(target, "wb") as handle:
        handle.write(b"icns" + struct.pack(">I", 8 + len(body)) + body)
    print(f"    {target}: {', '.join(kind.decode() for kind in sorted(entries))}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
