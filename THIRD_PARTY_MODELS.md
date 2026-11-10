# Third-Party Model Attributions & Licenses

Speechyyy provides optional local offline speech recognition models using INT8-quantized ONNX weights packaged by the [`k2-fsa/sherpa-onnx`](https://github.com/k2-fsa/sherpa-onnx) project. Each model is subject to its original author's license terms.

---

## Model Inventory

| Model ID | Architecture | Upstream Author | License | License URL | Upstream Repository / Model Card |
|---|---|---|---|---|---|
| `parakeet-tdt-v3` | NeMo Parakeet-TDT 0.6B v3 (INT8) | NVIDIA Corporation | **CC-BY-4.0** | [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/) | [NVIDIA NeMo Parakeet-TDT-0.6B-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) |
| `whisper-small` | Whisper Small (INT8) | OpenAI | **MIT** | [MIT License](https://github.com/openai/whisper/blob/main/LICENSE) | [OpenAI Whisper](https://github.com/openai/whisper) |
| `whisper-large-v3-turbo` | Whisper Large-v3-Turbo (INT8) | OpenAI | **MIT** | [MIT License](https://github.com/openai/whisper/blob/main/LICENSE) | [OpenAI Whisper](https://github.com/openai/whisper) |
| `moonshine-base` | Moonshine Base (INT8) | Useful Sensors, Inc. | **MIT** | [MIT License](https://github.com/usefulsensors/moonshine/blob/main/LICENSE) | [Useful Sensors Moonshine](https://github.com/usefulsensors/moonshine) |

---

## Detailed Attributions

### 1. Parakeet-TDT-v3 (`parakeet-tdt-v3`)
- **Original Model:** `nvidia/parakeet-tdt-0.6b-v3`
- **Authors:** NVIDIA NeMo Speech Team
- **License:** Creative Commons Attribution 4.0 International (CC-BY-4.0)
- **Attribution Statement:** "Parakeet-TDT-0.6b-v3 is developed and published by NVIDIA Corporation under CC-BY-4.0. Exported to ONNX and quantized to INT8 by k2-fsa/sherpa-onnx (Apache-2.0)."
- **Asset Release:** `https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2`

### 2. Whisper Small (`whisper-small`)
- **Original Model:** OpenAI Whisper Small
- **Authors:** OpenAI (Alec Radford, Jong Wook Kim, et al.)
- **License:** MIT License
- **Attribution Statement:** "Whisper Small is developed and published by OpenAI under the MIT License. Converted to ONNX and quantized by k2-fsa/sherpa-onnx (Apache-2.0)."
- **Asset Release:** `https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-small.tar.bz2`

### 3. Whisper Large-v3-Turbo (`whisper-large-v3-turbo`)
- **Original Model:** OpenAI Whisper Large-v3-Turbo
- **Authors:** OpenAI (Jong Wook Kim, et al.)
- **License:** MIT License
- **Attribution Statement:** "Whisper Large-v3-Turbo is developed and published by OpenAI under the MIT License. Converted to ONNX and quantized by k2-fsa/sherpa-onnx (Apache-2.0)."
- **Asset Release:** `https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-turbo.tar.bz2`

### 4. Moonshine Base (`moonshine-base`)
- **Original Model:** Moonshine Base (English)
- **Authors:** Useful Sensors, Inc.
- **License:** MIT License
- **Attribution Statement:** "Moonshine Base is developed and published by Useful Sensors, Inc. under the MIT License. Converted to ONNX and quantized to INT8 by k2-fsa/sherpa-onnx (Apache-2.0)."
- **Asset Release:** `https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-moonshine-base-en-int8.tar.bz2`

---

## Runtime Packaging Framework
All ONNX runtime binaries and packaging scripts provided by [`k2-fsa/sherpa-onnx`](https://github.com/k2-fsa/sherpa-onnx) are licensed under the **Apache-2.0** license.

