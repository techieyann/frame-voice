#!/usr/bin/env bash
# Build the pinned helper tools on a native Linux ARM64 builder.
set -euo pipefail
cd "$(dirname "$0")/.."
runtime_root="$PWD/.build/runtime"
mkdir -p "$runtime_root/bin" "$runtime_root/licenses" "$runtime_root/sources"

cmake -S .build/ydotool -B .build/ydotool/build -DCMAKE_BUILD_TYPE=Release
cmake --build .build/ydotool/build --parallel --target ydotool ydotoold
install -m 755 .build/ydotool/build/ydotool .build/ydotool/build/ydotoold "$runtime_root/bin/"

cmake -S .build/whisper -B .build/whisper/build -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=OFF -DGGML_BACKEND_DL=OFF -DGGML_NATIVE=OFF \
  -DGGML_OPENMP=OFF -DWHISPER_CURL=OFF -DWHISPER_BUILD_TESTS=OFF
cmake --build .build/whisper/build --parallel --target whisper-cli
install -m 755 .build/whisper/build/bin/whisper-cli "$runtime_root/bin/"

(
  cd .build/ffmpeg
  ./configure --disable-everything --disable-autodetect --disable-doc \
    --disable-shared --enable-static --enable-ffmpeg --disable-ffplay --disable-ffprobe \
    --enable-libpulse --enable-indev=pulse --enable-protocol=file,pipe \
    --enable-demuxer=wav --enable-muxer=wav,pcm_s16le \
    --enable-decoder=pcm_s16le,pcm_s32le,pcm_f32le --enable-encoder=pcm_s16le \
    --enable-filter=aresample,aformat,anull,abuffer,abuffersink,atrim
  make -j"$(nproc)" ffmpeg
)
install -m 755 .build/ffmpeg/ffmpeg "$runtime_root/bin/"
install -m 644 .build/ydotool/LICENSE "$runtime_root/licenses/ydotool.txt"
install -m 644 .build/whisper/LICENSE "$runtime_root/licenses/Whisper.txt"
install -m 644 .build/ffmpeg/COPYING.LGPLv2.1 "$runtime_root/licenses/FFmpeg.txt"
# Corresponding unmodified sources accompany the separately bundled helpers.
tar --exclude=.git --exclude=build -C .build -czf "$runtime_root/sources/ydotool-v1.0.4.tar.gz" ydotool
tar --exclude=.git --exclude=build -C .build -czf "$runtime_root/sources/whisper-v1.9.4.tar.gz" whisper
tar --exclude=.git --exclude=ffmpeg/ffmpeg --exclude='*.o' --exclude='*.a' --exclude='*.d' \
  -C .build -czf "$runtime_root/sources/ffmpeg-n8.0.3.tar.gz" ffmpeg
install -m 644 tools/build-runtime.sh "$runtime_root/sources/build-runtime.sh"
"$runtime_root/bin/ydotoold" --version
"$runtime_root/bin/whisper-cli" --help >/dev/null 2>&1
"$runtime_root/bin/ffmpeg" -version
python3 - <<'CHECK'
import re
import subprocess
from pathlib import Path
import wave
with wave.open('.build/runtime/smoke.wav', 'wb') as stream:
    stream.setnchannels(1); stream.setsampwidth(2); stream.setframerate(16000)
    stream.writeframes(b'\0' * 3200)
subprocess.run(['.build/runtime/bin/ffmpeg', '-nostdin', '-v', 'error', '-y', '-i',
                '.build/runtime/smoke.wav', '-f', 's16le', '.build/runtime/smoke.pcm'], check=True)
assert Path('.build/runtime/smoke.pcm').stat().st_size == 3200
result = subprocess.run(['.build/runtime/bin/ffmpeg', '-hide_banner', '-devices'], capture_output=True, text=True, check=True)
assert re.search(r'^\s*D\s+pulse\b', result.stdout + result.stderr, re.MULTILINE)
CHECK
