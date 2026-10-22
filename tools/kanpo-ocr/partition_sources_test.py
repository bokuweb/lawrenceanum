import copy
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from PIL import Image, ImageDraw
from decode_sources import extract, sha, write_json
from partition_sources import crop_frame, partition

ROOT = Path(__file__).resolve().parents[2]
EXTRACTOR = os.environ["KANPO_IMAGE_SOURCES"]


def source_pdf(stroke=True, clipped=False):
    image = Image.new("RGB", (40, 20), "white")
    draw = ImageDraw.Draw(image)
    for x in (3, 23):
        for y in (3, 12):
            draw.rectangle((x, y, x+13, y+3), fill="black")
    def stream(data, dictionary=""):
        return f"<< /Length {len(data)} {dictionary} >>\nstream\n".encode()+data+b"\nendstream"
    content = b"q 40 0 0 20 0 0 cm /Pic Do Q"
    if clipped:
        content = b"q 0 0 20 20 re W n 40 0 0 20 0 0 cm /Pic Do Q"
    if stroke:
        content += b" .5 w 20 2 m 20 18 l S"
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
               b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 40 20] /Contents 4 0 R /Resources << /XObject << /Pic 5 0 R >> >> >>",
               stream(content), stream(image.tobytes(), "/Type /XObject /Subtype /Image /Width 40 /Height 20 /ColorSpace /DeviceRGB /BitsPerComponent 8")]
    data, offsets = b"%PDF-1.4\n", [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(data))
        data += f"{i} 0 obj\n".encode()+obj+b"\nendobj\n"
    start = len(data)
    data += f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode()
    for offset in offsets[1:]:
        data += f"{offset:010} 00000 n \n".encode()
    return data+f"trailer << /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n".encode()


class PartitionSourceControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="kanpo-source-partitions-")
        self.directory = Path(self.temp.name)
        self.decoded, self.output = self.directory/"decoded", self.directory/"partitioned"
        self.geometry = self.directory/"geometry.json"

    def tearDown(self):
        self.temp.cleanup()

    def prepare(self, stroke=True, clipped=False):
        pdf = self.directory/"source.pdf"
        pdf.write_bytes(source_pdf(stroke, clipped))
        index = extract(pdf, EXTRACTOR, self.decoded)
        packet = json.loads((self.decoded/"sources.json").read_text())
        self.assertIsInstance(packet["pages"][0].get("native_paths"), list, "The native example must expose actual Page.paths for source-based partitioning")
        self.make_geometry()
        return index

    def make_geometry(self):
        module = ROOT/"tools/kanpo-ocr/partition.cjs"
        data = subprocess.check_output(["node", "-e", "process.stdout.write(JSON.stringify(require(process.argv[2]).geometryManifest(process.argv[1])))", str(self.decoded), str(module)])
        self.geometry.write_bytes(data)

    def test_real_native_stroke_creates_pixel_complete_source_bound_crops(self):
        original = self.prepare()
        result = partition(self.decoded, self.geometry, self.output)
        parent = result["rows"][0]
        self.assertEqual(parent["status"], "partitioned")
        self.assertEqual([p["source_crop_pixels"] for p in parent["partitions"]], [[0, 0, 20, 20], [20, 0, 40, 20]])
        before = Image.open(self.decoded/original["rows"][0]["PNG"]).convert("RGBA")
        rebuilt = Image.new("RGBA", before.size)
        for part in parent["partitions"]:
            receipt = json.loads((self.output/part["receipt"]).read_text())
            png = self.output/part["PNG"]
            self.assertEqual(receipt["PNG_sha256"], sha(png.read_bytes()))
            self.assertEqual(receipt["partition"], part["partition"])
            self.assertEqual(receipt["parent_PNG_sha256"], sha((self.decoded/original["rows"][0]["PNG"]).read_bytes()))
            self.assertEqual(receipt["visibility"], "unverified")
            rebuilt.paste(Image.open(png).convert("RGBA"), tuple(part["source_crop_pixels"][:2]))
        self.assertEqual(before.tobytes(), rebuilt.tobytes())
        self.assertFalse(result["body_coverage_verified"])
        self.assertIsNone(result["article_assignment"])

    def test_no_native_stroke_retains_exact_full_image_and_placement(self):
        original = self.prepare(stroke=False)
        result = partition(self.decoded, self.geometry, self.output)
        self.assertEqual(result["rows"][0]["status"], "decoded")
        for field in ("PNG", "placement", "receipt"):
            name = original["rows"][0][field]
            self.assertEqual((self.output/name).read_bytes(), (self.decoded/name).read_bytes())

    def test_original_four_column_page_splits_from_source_strokes_without_ocr_boxes(self):
        for name in ["20261007g002210006", "20260723h017530015"]:
            with self.subTest(original=name):
                pdf = ROOT/"crates/kanpo-pdf/src/image-source-fixtures"/(name+".pdf")
                decoded, output = self.directory/(name+"-decoded"), self.directory/(name+"-partitioned")
                original = extract(pdf, EXTRACTOR, decoded)
                previous = self.decoded
                self.decoded = decoded
                self.make_geometry()
                self.decoded = previous
                result = partition(decoded, self.geometry, output)
                row = result["rows"][0]
                self.assertEqual(row["status"], "partitioned")
                self.assertEqual(len(row["partitions"]), 4)
                before = Image.open(decoded/original["rows"][0]["PNG"]).convert("RGBA")
                rebuilt = Image.new("RGBA", before.size)
                for part in row["partitions"]:
                    image = Image.open(output/part["PNG"]).convert("RGBA")
                    rebuilt.paste(image, tuple(part["source_crop_pixels"][:2]))
                self.assertEqual(before.tobytes(), rebuilt.tobytes())
                self.assertTrue(row["all_source_RGBA_pixels_reconstruct_exactly"])
                self.assertEqual(json.loads((decoded/"sources.json").read_text())["pages"][0]["rotate"], 270)

    def test_changed_png_after_geometry_capture_is_a_retained_failure(self):
        original = self.prepare()
        with (self.decoded/original["rows"][0]["PNG"]).open("ab") as file:
            file.write(b"changed")
        result = partition(self.decoded, self.geometry, self.output)
        self.assertEqual(result["rows"][0]["status"], "failed")
        self.assertEqual(json.loads((self.output/"terminal.json").read_text())["status"], "partial")

    def test_changed_original_pdf_is_rejected_without_publishing_success(self):
        self.prepare()
        (self.decoded/"source.pdf").chmod(0o600)
        (self.decoded/"source.pdf").write_bytes(b"different original PDF")
        with self.assertRaisesRegex(ValueError, "PDF/source index"):
            partition(self.decoded, self.geometry, self.output)
        self.assertFalse((self.output/"index.json").exists())
        self.assertEqual(json.loads((self.output/"terminal.json").read_text())["status"], "failed")

    def test_existing_partition_run_cannot_be_replaced(self):
        self.prepare()
        partition(self.decoded, self.geometry, self.output)
        before = (self.output/"index.json").read_bytes()
        with self.assertRaises(FileExistsError):
            partition(self.decoded, self.geometry, self.output)
        self.assertEqual(before, (self.output/"index.json").read_bytes())

    def test_unknown_source_geometry_abstains_without_modifying_original_pixels(self):
        index = self.prepare()
        row = index["rows"][0]
        receipt_path = self.decoded/row["receipt"]
        receipt = json.loads(receipt_path.read_text())
        receipt["source_geometry_complete"] = False
        receipt_path.write_text(json.dumps(receipt))
        row["receipt_sha256"] = sha(receipt_path.read_bytes())
        (self.decoded/"index.json").write_text(json.dumps(index))
        self.make_geometry()
        result = partition(self.decoded, self.geometry, self.output)
        self.assertEqual(result["rows"][0]["status"], "decoded")
        plan = json.loads((self.output/result["rows"][0]["partition_plan"]).read_text())
        self.assertEqual(plan["reason"], "incomplete_source_geometry")
        self.assertEqual((self.output/row["PNG"]).read_bytes(), (self.decoded/row["PNG"]).read_bytes())

    def test_an_image_clip_is_not_replaced_by_the_intrinsic_white_gutter(self):
        original = self.prepare(clipped=True)
        receipt = json.loads((self.decoded/original["rows"][0]["receipt"]).read_text())
        self.assertTrue(receipt["clips"])
        result = partition(self.decoded, self.geometry, self.output)
        row = result["rows"][0]
        self.assertEqual(row["status"], "decoded")
        proposal = json.loads((self.output/row["partition_plan"]).read_text())
        self.assertEqual(proposal["reason"], "source_clipping_or_Form_not_composited")

    def test_crop_inverse_maps_to_the_same_page_corners_for_rotation_reflection_and_shear(self):
        frames = []
        for rotate in [0, 90, 180, 270]:
            for inverse in [[1, 0, 0, 1, 0, 0], [-1, 0, 0, 1, 80, 0], [0, 1, -1, 0, 80, 0], [.5, .25, .125, 2, 3, 7]]:
                frame = {"pixel_width": 80, "pixel_height": 100, "image_width": 200, "image_height": 300,
                         "recognition_to_image": inverse, "image_to_user": [240, 0, 0, 360, 11, 17], "crop_box": [7, 13, 500, 600], "rotate": rotate}
                box = [3, 5, 31, 44]
                frames.append({"parent": frame, "child": crop_frame(frame, box), "box": {"x0": box[0], "y0": box[1], "x1": box[2], "y1": box[3]}})
        data = self.directory/"frames.json"
        data.write_text(json.dumps(frames))
        module = ROOT/"tools/kanpo-ocr/placement.cjs"
        code = "const fs=require('node:fs'),assert=require('node:assert/strict'),{projectBox}=require(process.argv[2]);for(const f of JSON.parse(fs.readFileSync(process.argv[1]))){const a=projectBox(f.box,f.parent),b=projectBox({x0:0,y0:0,x1:f.child.pixel_width,y1:f.child.pixel_height},f.child);assert.deepEqual(a.quad_points,b.quad_points);}"
        subprocess.run(["node", "-e", code, str(data), str(module)], check=True)


if __name__ == "__main__":
    unittest.main()
