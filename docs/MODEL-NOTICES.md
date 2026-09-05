# Local model and optics assets

## YuNet face geometry (Phase 5)

Source: [OpenCV Zoo YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet). The model directory is MIT licensed; the exact license is bundled as `toolkit/YuNet-MIT.txt`. Model `face_detection_yunet_2023mar.onnx`, immutable revision `f12e12798e8314f7c074a6656816c048dcc95b7a`; SHA-256 `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4`, 232,589 bytes. License SHA-256 `c83b8120c50ccbd4c4f96edf53141bdd566ebb8f8e9227e415326aa1b1aba958`. Preparation verifies both; inference verifies the model.

The isolated helper uses existing ONNX Runtime 1.29.0 / ort 2.0.0-rc.13, CPU only. Output is face geometry/detection confidence, **not eyelid state, identity, demographics or attractiveness**. No eye-state weights were added. No user photos/crops/templates are uploaded or committed. See [AI_CULLING.md](AI_CULLING.md) for model limitations and explicit unavailable eye state.

## Existing Phase 2.1 assets

MODNet portrait matting: https://github.com/ZHKKKe/MODNet (Apache-2.0).
ONNX conversion: https://huggingface.co/Xenova/modnet (model card declares Apache-2.0).
Pinned conversion revision: fa2fa546052fba4c08921230a26cc69a333fca12.
FP32 model SHA-256: 07c308cf0fc7e6e8b2065a12ed7fc07e1de8febb7dc7839d7b7f15dd66584df9 (25,888,640 bytes).
The model predicts a portrait alpha matte, not a semantic object label or calibrated confidence score. No training or cloud inference is performed.

ONNX Runtime 1.29.0: https://github.com/microsoft/onnxruntime (MIT). The runtime LICENSE and ThirdPartyNotices accompany the binaries. CPU execution only in this phase.

Lensfun database: https://github.com/lensfun/lensfun, revision 23e8cb8050d680c7a293edb3d48b600754665f05. The unmodified XML database is CC BY-SA 3.0; its license is bundled. Attribution belongs to the Lensfun project and profile contributors recorded in the files. PhotoEditor implements a documented subset of the mathematical models in Rust; it does not link Lensfun's LGPL library. Unsupported or ambiguous calibration is not silently applied.

Downloaded weights, database and native binaries are ignored build resources, not source-control payloads. Preparation verifies pinned hashes. No application-runtime model download is performed. Review all notices and license obligations before commercial distribution; no installer/signing is implemented here.
