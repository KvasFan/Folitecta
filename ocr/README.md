# Text recognition models

Folitecta recognizes text (OCR) with two neural network models of the PaddleOCR project, in the
ONNX format, run by the pure-Rust inference library [tract](https://github.com/sonos/tract).
They are separate data files read when text is first recognized; the release packages carry
them in an `ocr` folder next to the program.

| File | What it does | Origin | SHA-256 |
|---|---|---|---|
| `det.onnx` | Finds the lines of text in a picture (PP-OCRv4 mobile detection model) | PaddleOCR, converted to ONNX by the RapidOCR project (`SWHL/RapidOCR`, `PP-OCRv4/ch_PP-OCRv4_det_infer.onnx`) | `d2a7720d45a54257208b1e13e36a8479894cb74155a5efe29462512d42f49da9` |
| `rec-latin.onnx` | Reads a line of text in the Latin script: English, Italian, French, German, Spanish, Portuguese, Dutch, the Nordic languages, Polish, Czech and 20 more (PP-OCRv5 mobile Latin recognition model) | PaddleOCR, converted to ONNX (`monkt/paddleocr-onnx`, `languages/latin/rec.onnx`) | `614ffc2d6d3902d360fad7f1b0dd455ee45e877069d14c4e51a99dc4ef144409` |
| `dict-latin.txt` | The characters the recognition model reads, one per line | the same (`languages/latin/dict.txt`) | `3c0a8a79b612653c25f765271714f71281e4e955962c153e272b7b8c1d2b13ff` |

PaddleOCR (https://github.com/PaddlePaddle/PaddleOCR) and its models are Copyright PaddlePaddle
Authors and released under the Apache License 2.0; the ONNX conversions are distributed under the
same licence. The full licence text is in [LICENSE-Apache-2.0.txt](LICENSE-Apache-2.0.txt). The
files are unchanged copies of the ones named above.
