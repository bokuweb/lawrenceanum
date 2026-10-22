"""Opt-in intrinsic image decoding. This is not a PDF compositor or OCR engine."""
import argparse
import hashlib
import importlib.metadata
import json
import math
import os
import subprocess
import uuid
from io import BytesIO
from pathlib import Path

from PIL import Image
from pypdf import PdfReader
from pypdf.generic import (
    ArrayObject, BooleanObject, ByteStringObject, DictionaryObject, FloatObject,
    IndirectObject, NameObject, NullObject, NumberObject, TextStringObject,
)

VERSIONS = {name: importlib.metadata.version(name) for name in ("pypdf", "Pillow", "cryptography")}
EXPECTED_VERSIONS = {"pypdf": "6.10.0", "Pillow": "12.3.0", "cryptography": "50.0.2"}
IMAGE_KEYS = (
    "Width", "Height", "BitsPerComponent", "ColorSpace", "Filter", "Decode",
    "DecodeParms", "ImageMask", "Mask", "SMask", "SMaskInData", "Matte",
    "Intent", "Interpolate",
)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    temporary = path.with_name(path.name+"."+str(uuid.uuid4())+".tmp")
    try:
        with temporary.open("x", encoding="utf-8") as file:
            json.dump(value, file, ensure_ascii=False, indent=2, allow_nan=False)
            file.write("\n")
            file.flush()
            os.fsync(file.fileno())
        os.link(temporary, path)
        temporary.unlink()
    except Exception as error:
        error.partial_output = str(temporary)
        raise


def record_failure(output, error, writer=write_json):
    if not (output/"terminal.json").exists():
        try:
            writer(output/"terminal.json", {"status": "failed", "error": str(error),
                   "partial_output": getattr(error, "partial_output", None), "canonical_integration": False})
        except Exception as recording_error:
            error.recording_error = str(recording_error)
    return error


def canonical(value):
    """Compare declared PDF values without resolving indirect references."""
    if isinstance(value, IndirectObject):
        return {"type": "ref", "object": value.idnum, "generation": value.generation}
    if isinstance(value, NullObject):
        return {"type": "null"}
    if isinstance(value, BooleanObject):
        return {"type": "bool", "value": value.value}
    if isinstance(value, FloatObject):
        return {"type": "real", "value": float(value)}
    if isinstance(value, NumberObject):
        return {"type": "int", "value": int(value)}
    if isinstance(value, NameObject):
        # Non-ASCII names can have ambiguous round trips through a second parser.
        # Abstain rather than assume a UTF-8 encoding for original PDF bytes.
        return {"type": "name", "hex": str(value)[1:].encode("ascii").hex()}
    if isinstance(value, (ByteStringObject, TextStringObject)):
        return {"type": "string", "hex": value.original_bytes.hex()}
    if isinstance(value, ArrayObject):
        return {"type": "array", "values": [canonical(v) for v in value]}
    if isinstance(value, DictionaryObject):
        return {"type": "dict", "entries": sorted(
            [[str(k)[1:].encode("ascii").hex(), canonical(v)] for k, v in value.items()]
        )}
    raise ValueError(f"Unsupported declared PDF value: {type(value).__name__}")


def page_vector(vector, rotate):
    x, y = vector
    return {0: (x, -y), 90: (y, x), 180: (-x, y), 270: (-y, -x)}[rotate]


def orient(image, matrix, rotate):
    """Lossless dihedral orientation only; retain intrinsic axes for shear/obliquity."""
    if len(matrix) != 6 or not all(isinstance(v, (int, float)) and math.isfinite(v) for v in matrix):
        raise ValueError("Invalid effective image matrix")
    a, b, c, d, _, _ = matrix
    if not math.isfinite(a*d-b*c) or a*d-b*c == 0:
        raise ValueError("Unrepresentable or singular image matrix")
    w, h = image.size
    vx = page_vector((a/w, b/w), rotate)
    vy = page_vector((-c/h, -d/h), rotate)
    if vx[1] == 0 and vy[0] == 0 and vx[0] != 0 and vy[1] != 0:
        sx, sy = (1 if vx[0] > 0 else -1), (1 if vy[1] > 0 else -1)
        output = image.copy()
        inverse = [sx, 0, 0, sy, 0 if sx > 0 else w, 0 if sy > 0 else h]
        footprint = (abs(vx[0]), abs(vy[1]))
    elif vx[0] == 0 and vy[1] == 0 and vx[1] != 0 and vy[0] != 0:
        sx, sy = (1 if vy[0] > 0 else -1), (1 if vx[1] > 0 else -1)
        output = image.transpose(Image.Transpose.TRANSPOSE)
        inverse = [0, sx, sy, 0, 0 if sy > 0 else w, 0 if sx > 0 else h]
        footprint = (abs(vy[0]), abs(vx[1]))
    else:
        return image.copy(), [1, 0, 0, 1, 0, 0], "intrinsic_axes", (math.hypot(*vx), math.hypot(*vy))
    if sx < 0:
        output = output.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    if sy < 0:
        output = output.transpose(Image.Transpose.FLIP_TOP_BOTTOM)
    return output, inverse, "display_axes_exact", footprint


def checked_object(reader, page, draw, digest):
    if page["document_sha256"] != digest:
        raise ValueError("Native packet does not match original PDF snapshot")
    ref = draw["object_ref"]
    if not isinstance(ref, list) or len(ref) != 2 or not all(type(v) is int and v >= 0 for v in ref):
        raise ValueError("An indirect image reference is required")
    obj = reader.get_object(IndirectObject(*ref, reader))
    # _data is version-pinned: compare after PDF decryption, before image filters.
    if not hasattr(obj, "_data") or sha(obj._data) != draw["encoded_sha256"] or len(obj._data) != draw["encoded_bytes"]:
        raise ValueError("Encoded image payload does not match native observation")
    if obj.get("/Subtype") != "/Image":
        raise ValueError("Observed reference is not an image")
    declared = dict(draw["declared_dictionary"]["entries"])
    for name in IMAGE_KEYS:
        actual = canonical(obj.raw_get("/"+name)) if "/"+name in obj else None
        if declared.get(name.encode("ascii").hex()) != actual:
            raise ValueError(f"Declared image dictionary mismatch: {name}")
    return obj


def decode_draw(reader, page, draw, digest, directory, packet_sha, extractor_sha):
    obj = checked_object(reader, page, draw, digest)
    pdf_page = reader.pages[page["page"]]
    if pdf_page.user_unit != 1:
        raise ValueError("UserUnit other than1 is not represented by this native packet")
    if list(map(float, pdf_page.cropbox)) != page["crop_box"] or pdf_page.rotation % 360 != page["rotate"]:
        raise ValueError("Page CropBox/Rotate disagrees with native observation")
    image = obj.decode_as_image()
    if image is None or list(image.size) != [draw["pixel_width"], draw["pixel_height"]]:
        raise ValueError("Decoded dimensions do not match native observation")
    output, inverse, orientation, (sx, sy) = orient(image, draw["image_to_user"], page["rotate"])
    physical = [output.width*sx, output.height*sy]
    if not all(math.isfinite(v) and v > 0 for v in [sx, sy, *physical, 72/sx, 72/sy]):
        raise ValueError("Unrepresentable source sampling")
    dpi = round(72/sy)
    if dpi <= 0 or dpi > 2**53-1:
        raise ValueError("Source sampling cannot be represented as a positive integer DPI")
    prefix = f"page{page['page']}-draw{draw['draw']}"
    png = directory/(prefix+".png")
    with png.open("xb") as file:
        output.save(file, format="PNG")
    frame = {"pixel_width": output.width, "pixel_height": output.height,
             "image_width": image.width, "image_height": image.height,
             "recognition_to_image": inverse, "image_to_user": draw["image_to_user"],
             "crop_box": page["crop_box"], "rotate": page["rotate"]}
    placement = directory/(prefix+".placement.json")
    write_json(placement, frame)
    rgba = output.convert("RGBA")
    receipt = {"document_sha256": digest, "native_packet_sha256": packet_sha,
               "extractor_sha256": extractor_sha, "page": page["page"], "draw": draw["draw"],
               "object_ref": draw["object_ref"], "source_draw": draw,
               "matched_declared_image_keys": list(IMAGE_KEYS), "decoder_versions": VERSIONS,
               "PNG_sha256": sha(png.read_bytes()), "RGBA_pixels_sha256": sha(rgba.tobytes()),
               "placement_sha256": sha(placement.read_bytes()),
               "decoded_opaque_white": all(pair == (255, 255) for pair in rgba.getextrema()),
               "orientation": orientation, "pixel_footprint_points": physical,
               "source_dpi_xy": [72/sx, 72/sy], "model_dpi": dpi,
               "source_geometry_complete": draw["source_geometry_complete"],
               "capture_gaps": page["gaps"], "clips": draw["clips"],
               "visibility": "unverified", "article_assignment": None, "native_glyph_binding": None}
    receipt_path = directory/(prefix+".receipt.json")
    write_json(receipt_path, receipt)
    return {"page": page["page"], "draw": draw["draw"], "status": "decoded",
            "PNG": png.name, "placement": placement.name, "receipt": receipt_path.name,
            "receipt_sha256": sha(receipt_path.read_bytes()),
            "decoded_opaque_white": receipt["decoded_opaque_white"],
            "width_points": physical[0], "height_points": physical[1], "dpi": dpi}


def extract(pdf, extractor, output):
    if VERSIONS != EXPECTED_VERSIONS:
        raise ValueError(f"Expected decoder versions {EXPECTED_VERSIONS}; received {VERSIONS}")
    snapshot, executable = Path(pdf).read_bytes(), Path(extractor).read_bytes()
    output = Path(output)
    output.mkdir()  # Never replace an existing run, even a failed one.
    try:
        source = output/"source.pdf"
        source.write_bytes(snapshot)
        source.chmod(0o400)
        producer = output/"image-sources-executable"
        producer.write_bytes(executable)
        producer.chmod(0o500)
        with (output/"sources.json").open("xb") as stdout, (output/"extractor.stderr").open("xb") as stderr:
            subprocess.run([str(producer.resolve()), str(source.resolve())], stdout=stdout, stderr=stderr, check=True)
        if sha(source.read_bytes()) != sha(snapshot):
            raise ValueError("Extractor changed original PDF snapshot")
        packet_bytes = (output/"sources.json").read_bytes()
        packet = json.loads(packet_bytes)
        if packet.get("schema") != 1 or packet.get("source") != "native_xobject_snapshot":
            raise ValueError("Unsupported native source packet")
        reader = PdfReader(BytesIO(snapshot))
        if len(packet["pages"]) != len(reader.pages):
            raise ValueError("Native page count disagrees with PDF snapshot")
        rows = []
        for pi, page in enumerate(packet["pages"]):
            if page["page"] != pi or page["document_sha256"] != sha(snapshot):
                raise ValueError("Native page does not match original PDF snapshot")
            for di, draw in enumerate(page["images"]):
                if draw["draw"] != di:
                    raise ValueError("Image draw sequence is not canonical")
                try:
                    rows.append(decode_draw(reader, page, draw, sha(snapshot), output, sha(packet_bytes), sha(executable)))
                except Exception as error:
                    rows.append({"page": pi, "draw": di, "status": "failed", "error": str(error), "source_draw": draw})
        index = {"schema": 1, "kind": "intrinsic_pdf_image_sources", "document_sha256": sha(snapshot),
                 "extractor_sha256": sha(executable), "native_packet_sha256": sha(packet_bytes),
                 "decoder_sha256": sha(Path(__file__).read_bytes()), "decoder_versions": VERSIONS,
                 "pages": [{"page": p["page"], "capture_gaps": p["gaps"],
                            "visibility": "unverified"} for p in packet["pages"]],
                 "rows": rows, "visibility": "unverified", "body_coverage_verified": False,
                 "article_assignment": None, "native_glyph_binding": None}
        write_json(output/"index.json", index)
        write_json(output/"terminal.json", {"status": "partial" if any(r["status"] == "failed" for r in rows) else "complete",
                                            "decoded": sum(r["status"] == "decoded" for r in rows), "canonical_integration": False})
        return index
    except Exception as error:
        record_failure(output, error)
        raise


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--extractor", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("pdf")
    options = parser.parse_args()
    result = extract(options.pdf, options.extractor, options.output)
    raise SystemExit(2 if any(row["status"] == "failed" for row in result["rows"]) else 0)
