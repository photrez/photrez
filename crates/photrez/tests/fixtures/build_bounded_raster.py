from hashlib import sha256
from pathlib import Path
from struct import pack
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo
from zlib import compress, crc32

ROOT = Path(__file__).resolve().parent
RGBA = bytes([12, 34, 56, 255, 200, 150, 100, 255])
DOCUMENT = (
    b'{"format":"photrez-ptz","version":3,"id":"parity-doc","name":"Parity fixture",'
    b'"width":2,"height":1,"layers":[{"id":"raster-1","name":"Raster",'
    b'"type":"raster","visible":true,"opacity":1.0,"locked":false,'
    b'"blendMode":"normal","transform":{"x":0.0,"y":0.0,"scaleX":1.0,'
    b'"scaleY":1.0,"rotation":0.0,"flipH":false,"flipV":false},'
    b'"width":2,"height":1}]}'
)


def png_chunk(tag, data):
    checksum = crc32(tag + data) & 0xFFFFFFFF
    return pack(">I", len(data)) + tag + data + pack(">I", checksum)


PNG = (
    b"\x89PNG\r\n\x1a\n"
    + png_chunk(b"IHDR", pack(">IIBBBBB", 2, 1, 8, 6, 0, 0, 0))
    + png_chunk(b"IDAT", compress(b"\x00" + RGBA, 6))
    + png_chunk(b"IEND", b"")
)

FILES = {
    "bounded-raster.document.json": DOCUMENT,
    "bounded-raster.layer.png": PNG,
    "bounded-raster.rgba": RGBA,
}
for name, contents in FILES.items():
    (ROOT / name).write_bytes(contents)

with ZipFile(ROOT / "bounded-raster.ptz", "w", ZIP_DEFLATED, compresslevel=6) as archive:
    for name, contents in (("document.json", DOCUMENT), ("layers/raster-1.png", PNG)):
        info = ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
        info.compress_type = ZIP_DEFLATED
        info.create_system = 0
        info.external_attr = 0o600 << 16
        info.flag_bits = 0
        archive.writestr(info, contents, compress_type=ZIP_DEFLATED, compresslevel=6)

for path in sorted(ROOT.glob("bounded-raster.*")):
    contents = path.read_bytes()
    print(f"{path.name}: {len(contents)} bytes sha256={sha256(contents).hexdigest()}")
