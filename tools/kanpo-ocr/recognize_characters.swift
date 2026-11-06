import Foundation
import Vision
import ImageIO

// No manuscript, vocabulary, or language correction is supplied to the model.
var outputs: [[String: Any]] = []
for path in CommandLine.arguments.dropFirst() {
    let request = VNRecognizeTextRequest()
    request.revision = VNRecognizeTextRequestRevision3
    request.recognitionLevel = .accurate
    request.recognitionLanguages = ["ja-JP"]
    request.usesLanguageCorrection = false
    request.automaticallyDetectsLanguage = false
    request.minimumTextHeight = 0
    do { try VNImageRequestHandler(url: URL(fileURLWithPath: path), options: [:]).perform([request]) }
    catch { outputs.append(["path": path, "error": String(describing: error)]); continue }
    let observations = (request.results ?? []).map { observation -> [String: Any] in
        let candidates = observation.topCandidates(3).map { candidate -> [String: Any] in
            let text = candidate.string
            var characters: [[String: Any]] = []
            var offset = 0
            for start in text.indices {
                let end = text.index(after: start)
                let character = String(text[start..<end])
                var item: [String: Any] = ["text": character, "unicode_scalar_offset": offset,
                                          "unicode_scalar_length": character.unicodeScalars.count]
                if let box = try? candidate.boundingBox(for: start..<end) {
                    let b = box.boundingBox
                    item["bbox"] = [b.minX, b.minY, b.maxX, b.maxY]
                    item["quad"] = [[box.topLeft.x, box.topLeft.y], [box.topRight.x, box.topRight.y],
                                     [box.bottomRight.x, box.bottomRight.y], [box.bottomLeft.x, box.bottomLeft.y]]
                } else { item["geometry"] = "unavailable" }
                characters.append(item)
                offset += character.unicodeScalars.count
            }
            return ["text": text, "confidence": candidate.confidence, "characters": characters]
        }
        let b = observation.boundingBox
        return ["bbox": [b.minX, b.minY, b.maxX, b.maxY], "candidates": candidates]
    }
    outputs.append(["path": path, "revision": request.revision,
                    "language_correction": false, "observations": observations])
}
let data = try JSONSerialization.data(withJSONObject: [
    "os": ProcessInfo.processInfo.operatingSystemVersionString, "outputs": outputs
], options: [.prettyPrinted, .sortedKeys])
FileHandle.standardOutput.write(data)
