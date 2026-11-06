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
    try VNImageRequestHandler(url: URL(fileURLWithPath: path), options: [:]).perform([request])
    let observations = (request.results ?? []).map { observation -> [String: Any] in
        let candidates = observation.topCandidates(3).map {
            ["text": $0.string, "confidence": $0.confidence] as [String: Any]
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
