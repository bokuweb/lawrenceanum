"""Source-preserving intrinsic image partitions; called by the Node geometry producer."""
import argparse
import json
import math
from io import BytesIO
from pathlib import Path

from PIL import Image
from decode_sources import EXPECTED_VERSIONS, VERSIONS, record_failure, sha, write_json
from source_gutters import plan


def asset(directory, name):
    if not isinstance(name, str) or Path(name).name != name or name in (".", ".."):
        raise ValueError("Expected a local source asset filename")
    return (directory/name).read_bytes()


def crop_frame(frame, box):
    result = dict(frame)
    a, b, c, d, e, f = frame["recognition_to_image"]
    x, y, right, bottom = box
    result["recognition_to_image"] = [a, b, c, d, a*x+c*y+e, b*x+d*y+f]
    if not all(type(v) in (int, float) and math.isfinite(v) for v in result["recognition_to_image"]):
        raise ValueError("Unrepresentable crop transform")
    result["pixel_width"], result["pixel_height"] = right-x, bottom-y
    return result


def partition(input_directory, geometry_path, output_directory):
    if VERSIONS != EXPECTED_VERSIONS:
        raise ValueError("Unexpected source decoder versions")
    source, output = Path(input_directory), Path(output_directory)
    pdf = asset(source, "source.pdf")
    index_bytes, packet_bytes = asset(source, "index.json"), asset(source, "sources.json")
    geometry_bytes = Path(geometry_path).read_bytes()
    index, packet, geometry = json.loads(index_bytes), json.loads(packet_bytes), json.loads(geometry_bytes)
    output.mkdir()
    try:
        digest = sha(pdf)
        if index["kind"] != "intrinsic_pdf_image_sources" or index["document_sha256"] != digest or index["native_packet_sha256"] != sha(packet_bytes):
            raise ValueError("Original PDF/source index mismatch")
        if geometry.get("kind") != "source_canvas_geometry" or geometry.get("schema") != 1 or geometry["document_sha256"] != digest or geometry["decoded_index_sha256"] != sha(index_bytes) or geometry["native_packet_sha256"] != sha(packet_bytes):
            raise ValueError("Geometry manifest does not match decoded source")
        for name, data in [("source.pdf", pdf), ("sources.json", packet_bytes), ("decoded-index.json", index_bytes), ("source-geometry.json", geometry_bytes)]:
            with (output/name).open("xb") as file:
                file.write(data)
        (output/"source.pdf").chmod(0o400)
        frames = {}
        for row in geometry["rows"]:
            key = (row["page"], row["draw"])
            if key in frames:
                raise ValueError("Ambiguous source geometry occurrence")
            frames[key] = row
        rows = []
        for entry in index["rows"]:
            if entry["status"] == "failed":
                rows.append(entry)
                continue
            if entry["status"] != "decoded" or not all(type(entry[k]) is int and entry[k] >= 0 for k in ("page", "draw")):
                raise ValueError("Invalid source draw occurrence")
            try:
                pi, di = entry["page"], entry["draw"]
                page = packet["pages"][pi]
                if page["page"] != pi or page["document_sha256"] != digest or not isinstance(page.get("native_paths"), list):
                    raise ValueError("Source page/native paths do not match PDF snapshot")
                png = asset(source, entry["PNG"])
                frame_bytes, receipt_bytes = asset(source, entry["placement"]), asset(source, entry["receipt"])
                frame, receipt, bound = json.loads(frame_bytes), json.loads(receipt_bytes), frames[(pi, di)]
                if sha(receipt_bytes) != entry["receipt_sha256"] or receipt["document_sha256"] != digest or receipt["native_packet_sha256"] != sha(packet_bytes) or receipt["source_draw"] != page["images"][di]:
                    raise ValueError("Image receipt does not match native draw")
                if receipt["page"] != pi or receipt["draw"] != di or receipt["PNG_sha256"] != sha(png) or receipt["placement_sha256"] != sha(frame_bytes):
                    raise ValueError("Source image/placement hashes disagree")
                draw = page["images"][di]
                if frame["image_to_user"] != draw["image_to_user"] or [frame["image_width"], frame["image_height"]] != [draw["pixel_width"], draw["pixel_height"]] or frame["crop_box"] != page["crop_box"] or frame["rotate"] != page["rotate"]:
                    raise ValueError("Source placement differs from native image draw/page")
                if [entry["width_points"], entry["height_points"]] != receipt["pixel_footprint_points"] or entry["dpi"] != receipt["model_dpi"]:
                    raise ValueError("Recognition sampling differs from source receipt")
                if bound["PNG_sha256"] != sha(png) or bound["placement_sha256"] != sha(frame_bytes) or bound["receipt_sha256"] != sha(receipt_bytes):
                    raise ValueError("Projected canvas does not match source draw")
                image = Image.open(BytesIO(png))
                if list(image.size) != [frame["pixel_width"], frame["pixel_height"]]:
                    raise ValueError("Source canvas dimensions disagree")
                prefix = f"page{pi}-draw{di}"
                for name, data in [(entry["PNG"], png), (entry["placement"], frame_bytes), (entry["receipt"], receipt_bytes)]:
                    with (output/name).open("xb") as file:
                        file.write(data)
                if bound["projection_status"] != "projected" or receipt["source_geometry_complete"] is not True or draw["source_geometry_complete"] is not True:
                    proposal = {"status": "full_image", "reason": "incomplete_source_geometry", "cuts": [], "rectangles": [[0, 0, image.width, image.height]]}
                elif receipt["clips"] or draw["clips"] or draw["form_path"]:
                    proposal = {"status": "full_image", "reason": "source_clipping_or_Form_not_composited", "cuts": [], "rectangles": [[0, 0, image.width, image.height]]}
                else:
                    proposal = plan(image, page["native_paths"], bound["page_quad"], "horizontal_ltr")
                plan_path = output/(prefix+"-partition-plan.json")
                write_json(plan_path, proposal)
                if proposal["status"] != "split":
                    rows.append({**entry, "partition_plan": plan_path.name})
                    continue
                rebuilt = Image.new("RGBA", image.size)
                parts = []
                for number, box in enumerate(proposal["rectangles"]):
                    crop = image.crop(box)
                    rebuilt.paste(crop.convert("RGBA"), (box[0], box[1]))
                    part_prefix = prefix+f"-part{number}"
                    png_path = output/(part_prefix+".png")
                    with png_path.open("xb") as file:
                        crop.save(file, format="PNG")
                    placement = output/(part_prefix+".placement.json")
                    write_json(placement, crop_frame(frame, box))
                    physical = [entry["width_points"]*crop.width/image.width, entry["height_points"]*crop.height/image.height]
                    derived = {**receipt, "partition": number, "PNG_sha256": sha(png_path.read_bytes()),
                               "RGBA_pixels_sha256": sha(crop.convert("RGBA").tobytes()),
                               "placement_sha256": sha(placement.read_bytes()), "pixel_footprint_points": physical,
                               "parent_image_receipt": entry["receipt"], "parent_image_receipt_sha256": sha(receipt_bytes),
                               "parent_PNG": entry["PNG"], "parent_PNG_sha256": sha(png),
                               "source_crop_pixels": box, "partition_plan_sha256": sha(plan_path.read_bytes()),
                               "partition_source_code_sha256": sha(Path(__file__).with_name("source_gutters.py").read_bytes()),
                               "partition_geometry_sha256": sha(geometry_bytes), "visibility": "unverified"}
                    receipt_path = output/(part_prefix+".receipt.json")
                    write_json(receipt_path, derived)
                    parts.append({"page": pi, "draw": di, "partition": number, "status": "decoded",
                                  "PNG": png_path.name, "placement": placement.name, "receipt": receipt_path.name,
                                  "receipt_sha256": sha(receipt_path.read_bytes()), "source_crop_pixels": box,
                                  "decoded_opaque_white": all(pair == (255, 255) for pair in crop.convert("RGBA").getextrema()),
                                  "width_points": physical[0], "height_points": physical[1], "dpi": entry["dpi"]})
                if rebuilt.tobytes() != image.convert("RGBA").tobytes():
                    raise ValueError("Partitions do not reconstruct original source pixels")
                rows.append({**entry, "status": "partitioned", "partition_plan": plan_path.name,
                             "all_source_RGBA_pixels_reconstruct_exactly": True, "partitions": parts})
            except Exception as error:
                rows.append({**entry, "status": "failed", "error": str(error)})
        result = {**index, "kind": "partitioned_pdf_image_sources", "rows": rows,
                  "partition_mode": "native-gutters-ltr", "source_index_sha256": sha(index_bytes),
                  "partition_geometry_sha256": sha(geometry_bytes), "partition_helper_sha256": sha(Path(__file__).read_bytes()),
                  "source_gutter_code_sha256": sha(Path(__file__).with_name("source_gutters.py").read_bytes()),
                  "body_coverage_verified": False, "visibility": "unverified", "article_assignment": None, "native_glyph_binding": None}
        write_json(output/"index.json", result)
        write_json(output/"terminal.json", {"status": "partial" if any(row["status"] == "failed" for row in rows) else "complete",
                                            "canonical_integration": False})
        return result
    except Exception as error:
        record_failure(output, error)
        raise


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True)
    parser.add_argument("--geometry", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    index = partition(args.input, args.geometry, args.output)
    raise SystemExit(2 if any(row["status"] == "failed" for row in index["rows"]) else 0)
