import copy
import json
import math
import os
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from io import BytesIO
from pathlib import Path

from PIL import Image
from pypdf import PdfReader
import decode_sources as decoder

ROOT = Path(__file__).resolve().parents[2]
EXTRACTOR = os.environ["KANPO_IMAGE_SOURCES"]  # Required; never silently skip native integration.


def stream(data, dictionary=""):
    return f"<< /Length {len(data)} {dictionary} >>\nstream\n".encode()+data+b"\nendstream"


def pdf(content=b"q 12 0 0 18 5 7 cm /Pic Do Q", extra_page="", image_dict="", data=None):
    data = bytes(range(18)) if data is None else data
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>",
               b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
               f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 200] /Contents 4 0 R /Resources << /XObject << /Pic 5 0 R >> >> {extra_page} >>".encode(),
               stream(content),
               stream(data, "/Type /XObject /Subtype /Image /Width 2 /Height 3 /ColorSpace /DeviceRGB /BitsPerComponent 8 "+image_dict)]
    output = b"%PDF-1.4\n"
    offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(output))
        output += f"{i} 0 obj\n".encode()+obj+b"\nendobj\n"
    start = len(output)
    output += f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode()
    for offset in offsets[1:]:
        output += f"{offset:010} 00000 n \n".encode()
    return output+f"trailer << /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n".encode()


class DecoderTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="kanpo-pdf-images-")
        self.directory = Path(self.temp.name)
        self.source = self.directory/"original.pdf"
        self.output = self.directory/"decoded"

    def tearDown(self):
        self.temp.cleanup()

    def extract(self, data=None):
        self.source.write_bytes(pdf() if data is None else data)
        return decoder.extract(self.source, EXTRACTOR, self.output)

    def test_actual_payload_png_and_placement_are_bound_to_original_snapshot(self):
        index = self.extract()
        row = index["rows"][0]
        self.assertEqual(row["status"], "decoded")
        self.assertEqual(index["document_sha256"], decoder.sha(self.source.read_bytes()))
        receipt = json.loads((self.output/row["receipt"]).read_text())
        self.assertEqual(receipt["PNG_sha256"], decoder.sha((self.output/row["PNG"]).read_bytes()))
        self.assertEqual(receipt["placement_sha256"], decoder.sha((self.output/row["placement"]).read_bytes()))
        image = Image.open(self.output/row["PNG"])
        self.assertEqual(image.tobytes(), bytes(range(18)))
        self.assertEqual(receipt["visibility"], "unverified")
        self.assertIsNone(receipt["article_assignment"])
        self.assertFalse(index["body_coverage_verified"])

    def test_same_image_used_twice_retains_two_draws_and_matrices(self):
        index = self.extract(pdf(b"q 12 0 0 18 5 7 cm /Pic Do Q q 24 0 0 36 31 41 cm /Pic Do Q"))
        self.assertEqual(len(index["rows"]), 2)
        receipts = [json.loads((self.output/r["receipt"]).read_text()) for r in index["rows"]]
        self.assertEqual(receipts[0]["object_ref"], receipts[1]["object_ref"])
        self.assertNotEqual(receipts[0]["source_draw"]["image_to_user"], receipts[1]["source_draw"]["image_to_user"])

    def test_unused_image_resource_is_not_invented_as_a_draw(self):
        index = self.extract(pdf(b""))
        self.assertEqual(index["rows"], [])
        self.assertFalse(index["body_coverage_verified"])

    def test_inline_image_gap_is_preserved_without_claiming_no_body(self):
        index = self.extract(pdf(b"BI /W 1 /H 1 /CS /G /BPC 8 ID \xff EI"))
        self.assertEqual(index["rows"], [])
        self.assertEqual(index["pages"][0]["capture_gaps"][0]["kind"], "InlineImage")
        self.assertFalse(index["body_coverage_verified"])

    def test_opaque_white_is_exact_pixel_property(self):
        index = self.extract(pdf(data=b"\xff"*18))
        self.assertTrue(index["rows"][0]["decoded_opaque_white"])
        rgba = Image.new("RGBA", (2, 3), (255, 255, 255, 0))
        self.assertFalse(all(p == (255, 255) for p in rgba.getextrema()))

    def test_user_unit_is_reported_as_unsupported_without_silent_point_scaling(self):
        index = self.extract(pdf(extra_page="/UserUnit 2"))
        self.assertEqual(index["rows"][0]["status"], "failed")
        self.assertIn("UserUnit", index["rows"][0]["error"])
        self.assertEqual(json.loads((self.output/"terminal.json").read_text())["status"], "partial")

    def test_nonzero_crop_and_quarter_rotation_remain_explicit(self):
        index = self.extract(pdf(extra_page="/CropBox [10 20 90 180] /Rotate 90"))
        row = index["rows"][0]
        self.assertEqual(row["status"], "decoded")
        frame = json.loads((self.output/row["placement"]).read_text())
        self.assertEqual(frame["crop_box"], [10, 20, 90, 180])
        self.assertEqual(frame["rotate"], 90)
        self.assertEqual([frame["pixel_width"], frame["pixel_height"]], [3, 2])
        self.assertEqual(frame["recognition_to_image"], [0, -1, 1, 0, 0, 3])

    def test_failing_native_extractor_retains_terminal_and_stderr(self):
        producer = self.directory/"failing-producer"
        producer.write_text("#!/usr/bin/env python3\nimport sys\nsys.stderr.write('native failure\\n')\nsys.exit(3)\n")
        self.source.write_bytes(pdf())
        with self.assertRaises(subprocess.CalledProcessError):
            decoder.extract(self.source, producer, self.output)
        self.assertEqual(json.loads((self.output/"terminal.json").read_text())["status"], "failed")
        self.assertIn("native failure", (self.output/"extractor.stderr").read_text())
        self.assertFalse((self.output/"index.json").exists())

    def test_decoder_cli_partial_failure_returns_two_with_a_retained_index(self):
        self.source.write_bytes(pdf(image_dict="/Filter /Unimplemented"))
        result = subprocess.run([sys.executable, str(Path(decoder.__file__)), "--extractor", EXTRACTOR,
                                 "--output", str(self.output), str(self.source)], capture_output=True)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(json.loads((self.output/"index.json").read_text())["rows"][0]["status"], "failed")

    def test_unsupported_image_filter_retains_failure_and_source_draw(self):
        index = self.extract(pdf(image_dict="/Filter /Unimplemented"))
        self.assertEqual(index["rows"][0]["status"], "failed")
        self.assertEqual(index["rows"][0]["source_draw"]["object_ref"], [5, 0])

    def test_existing_output_is_never_replaced(self):
        self.extract()
        before = (self.output/"index.json").read_bytes()
        with self.assertRaises(FileExistsError):
            decoder.extract(self.source, EXTRACTOR, self.output)
        self.assertEqual(before, (self.output/"index.json").read_bytes())

    def test_json_publication_is_complete_and_exclusive(self):
        target = self.directory/"record.json"
        decoder.write_json(target, {"original": True})
        with self.assertRaises(FileExistsError):
            decoder.write_json(target, {"replacement": True})
        self.assertEqual(json.loads(target.read_text()), {"original": True})

    def test_failed_json_publication_retains_partial_file_and_original_error(self):
        target = self.directory/"record.json"
        failure = OSError("publication failed")
        with patch.object(decoder.os, "link", side_effect=failure), self.assertRaises(OSError) as error:
            decoder.write_json(target, {"saved": True})
        self.assertIs(error.exception, failure)
        self.assertFalse(target.exists())
        self.assertEqual(json.loads(Path(failure.partial_output).read_text()), {"saved": True})
        self.assertIs(decoder.record_failure(self.directory, failure, writer=lambda *args: (_ for _ in ()).throw(OSError("journal failed"))), failure)
        self.assertEqual(failure.recording_error, "journal failed")

    def test_actual_encrypted_mixed_page_and_partial_images_decode(self):
        for name, count in [("20260925h017950022", 1), ("20260930g002150251", 3)]:
            with self.subTest(name=name):
                path = ROOT/"crates/kanpo-pdf/src/image-source-fixtures"/(name+".pdf")
                out = self.directory/name
                index = decoder.extract(path, EXTRACTOR, out)
                self.assertEqual(len(index["rows"]), count)
                self.assertTrue(all(r["status"] == "decoded" for r in index["rows"]))
                self.assertEqual(sum(r["decoded_opaque_white"] for r in index["rows"]), 0 if count == 1 else 1)

    def test_packet_pdf_payload_dictionary_reference_and_dimensions_mismatches_are_rejected(self):
        self.extract()
        packet = json.loads((self.output/"sources.json").read_text())
        original_page = packet["pages"][0]
        for field, value in [("encoded_sha256", "0"*64), ("encoded_bytes", 1), ("object_ref", [4, 0]), ("pixel_width", 9)]:
            page = copy.deepcopy(original_page)
            page["images"][0][field] = value
            with self.subTest(field=field), self.assertRaises(Exception):
                decoder.decode_draw(PdfReader(BytesIO(self.source.read_bytes())), page, page["images"][0], packet["pages"][0]["document_sha256"], self.directory, "packet", "extractor")
        page = copy.deepcopy(original_page)
        page["document_sha256"] = "0"*64
        with self.assertRaisesRegex(ValueError, "PDF snapshot"):
            decoder.checked_object(PdfReader(BytesIO(self.source.read_bytes())), page, page["images"][0], decoder.sha(self.source.read_bytes()))
        page = copy.deepcopy(original_page)
        entries = page["images"][0]["declared_dictionary"]["entries"]
        next(v for k, v in entries if k == b"ColorSpace".hex())["hex"] = b"DeviceGray".hex()
        with self.assertRaisesRegex(ValueError, "dictionary mismatch"):
            decoder.checked_object(PdfReader(BytesIO(self.source.read_bytes())), page, page["images"][0], page["document_sha256"])

    def test_all_dihedral_pixel_centers_round_trip_without_resampling(self):
        image = Image.new("L", (2, 3))
        image.putdata(range(6))
        for rotate in [0, 90, 180, 270]:
            for swap in [False, True]:
                for sx in [-1, 1]:
                    for sy in [-1, 1]:
                        matrix = [0, sx*8, sy*9, 0, 31, 41] if swap else [sx*8, 0, 0, sy*9, 31, 41]
                        output, inv, label, footprint = decoder.orient(image, matrix, rotate)
                        self.assertEqual(label, "display_axes_exact")
                        self.assertEqual(sorted(output.tobytes()), list(range(6)))
                        for y in range(output.height):
                            for x in range(output.width):
                                ix = inv[0]*(x+.5)+inv[2]*(y+.5)+inv[4]
                                iy = inv[1]*(x+.5)+inv[3]*(y+.5)+inv[5]
                                self.assertEqual(output.getpixel((x, y)), image.getpixel((math.floor(ix), math.floor(iy))))
                        self.assertTrue(all(v > 0 for v in footprint))

    def test_shear_keeps_intrinsic_pixels_and_does_not_guess_display_rotation(self):
        image = Image.new("L", (2, 3), 7)
        output, inv, label, _ = decoder.orient(image, [8, 2, 3, 9, 0, 0], 270)
        self.assertEqual(output.tobytes(), image.tobytes())
        self.assertEqual(inv, [1, 0, 0, 1, 0, 0])
        self.assertEqual(label, "intrinsic_axes")

    def test_singular_nonfinite_or_unrepresentable_matrix_is_rejected(self):
        for matrix in [[0]*6, [1, 0, 0, float("nan"), 0, 0], [1e308, 0, 0, 1e308, 0, 0]]:
            with self.subTest(matrix=matrix), self.assertRaises(ValueError):
                decoder.orient(Image.new("L", (2, 3)), matrix, 0)


if __name__ == "__main__":
    unittest.main()
