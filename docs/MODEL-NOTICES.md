# Local model and optics assets

MODNet portrait matting: https://github.com/ZHKKKe/MODNet (Apache-2.0).
ONNX conversion: https://huggingface.co/Xenova/modnet (model card declares Apache-2.0).
Pinned conversion revision: fa2fa546052fba4c08921230a26cc69a333fca12.
FP32 model SHA-256: 07c308cf0fc7e6e8b2065a12ed7fc07e1de8febb7dc7839d7b7f15dd66584df9 (25,888,640 bytes).
The model predicts a portrait alpha matte, not a semantic object label or calibrated confidence score. No training or cloud inference is performed.

ONNX Runtime 1.29.0: https://github.com/microsoft/onnxruntime (MIT). The runtime LICENSE and ThirdPartyNotices accompany the binaries. CPU execution only in this phase.

Lensfun database: https://github.com/lensfun/lensfun, revision 23e8cb8050d680c7a293edb3d48b600754665f05. The unmodified XML database is CC BY-SA 3.0; its license is bundled. Attribution belongs to the Lensfun project and profile contributors recorded in the files. PhotoEditor implements a documented subset of the mathematical models in Rust; it does not link Lensfun's LGPL library. Unsupported or ambiguous calibration is not silently applied.

Downloaded weights, database and native binaries are ignored build resources, not source-control payloads. Preparation verifies pinned hashes. No application-runtime model download is performed. Review all notices and license obligations before commercial distribution; no installer/signing is implemented here.
