#!/usr/bin/env bash
# Сборка APK КуМир для Android.
#
# Поддерживаются два окружения:
#   * Android SDK + NDK (Linux/macOS, CI);
#   * Termux на arm64 без NDK (как в исходном скрипте).
#
# Использование:
#   scripts/build-android.sh
#   ANDROID_TARGET=aarch64-linux-android scripts/build-android.sh
#
# Готовый APK: dist/android/kumir-ide-<версия>[-<abi>].apk

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ANDROID_DIR="$ROOT/android"
BUILD="$ANDROID_DIR/build"
DIST="${DIST:-$ROOT/dist}"
[[ "$DIST" == /* ]] || DIST="$ROOT/$DIST"

VERSION="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
IFS='.' read -r VER_MAJOR VER_MINOR VER_PATCH <<<"$VERSION"
VER_MAJOR="${VER_MAJOR:-0}"
VER_MINOR="${VER_MINOR:-0}"
VER_PATCH="${VER_PATCH:-0}"
VER_PATCH="${VER_PATCH%%-*}"
DEFAULT_VERSION_CODE=$((10#${VER_MAJOR} * 10000 + 10#${VER_MINOR} * 100 + 10#${VER_PATCH}))
VERSION_CODE="${ANDROID_VERSION_CODE:-$DEFAULT_VERSION_CODE}"

ANDROID_TARGET="${ANDROID_TARGET:-aarch64-linux-android}"
ANDROID_API="${ANDROID_API:-24}"
TARGET_SDK="${ANDROID_TARGET_SDK:-34}"
GAMES_VER="${ANDROID_GAMES_VERSION:-4.4.0}"

log()  { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

need() {
    local tool="$1" hint="${2:-}"
    command -v "$tool" >/dev/null 2>&1 || die \
        "не найдена команда «$tool».${hint:+ $hint}"
}

case "$ANDROID_TARGET" in
    aarch64-linux-android)
        ABI="arm64-v8a"; CLANG_TRIPLE="aarch64-linux-android"; LIBCXX_TRIPLE="aarch64-linux-android" ;;
    armv7-linux-androideabi)
        ABI="armeabi-v7a"; CLANG_TRIPLE="armv7a-linux-androideabi"; LIBCXX_TRIPLE="arm-linux-androideabi" ;;
    x86_64-linux-android)
        ABI="x86_64"; CLANG_TRIPLE="x86_64-linux-android"; LIBCXX_TRIPLE="x86_64-linux-android" ;;
    i686-linux-android)
        ABI="x86"; CLANG_TRIPLE="i686-linux-android"; LIBCXX_TRIPLE="i686-linux-android" ;;
    *)
        die "неподдерживаемая ANDROID_TARGET=$ANDROID_TARGET (aarch64-linux-android, armv7-linux-androideabi, x86_64-linux-android, i686-linux-android)" ;;
esac

find_sdk() {
    local candidate
    for candidate in \
        "${ANDROID_SDK_ROOT:-}" \
        "${ANDROID_HOME:-}" \
        "${PREFIX:-}/tmp/opencode/android/sdk" \
        "${PREFIX:-}/opt/android-sdk" \
        "$HOME/Android/Sdk" \
        "/opt/android-sdk" \
        "/usr/lib/android-sdk"; do
        [[ -n "$candidate" && -d "$candidate" ]] && { printf '%s\n' "$candidate"; return 0; }
    done
    return 1
}

find_ndk() {
    local candidate
    for candidate in "${ANDROID_NDK_HOME:-}" "${ANDROID_NDK_ROOT:-}" "${NDK_HOME:-}"; do
        [[ -n "$candidate" && -d "$candidate" ]] && { printf '%s\n' "$candidate"; return 0; }
    done
    if [[ -d "$SDK/ndk" ]]; then
        candidate="$(find "$SDK/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
        [[ -n "$candidate" ]] && { printf '%s\n' "$candidate"; return 0; }
    fi
    return 1
}

ensure_rust_target() {
    local libdir
    libdir="$(rustc --print target-libdir --target "$ANDROID_TARGET" 2>/dev/null || true)"
    if [[ -n "$libdir" && -d "$libdir" ]] && compgen -G "$libdir/libstd-*.rlib" >/dev/null; then
        return 0
    fi

    if command -v rustup >/dev/null 2>&1; then
        if ! rustup target list --installed | grep -qx "$ANDROID_TARGET"; then
            log "устанавливаю цель Rust: $ANDROID_TARGET"
            rustup target add "$ANDROID_TARGET" || die "не удалось установить цель $ANDROID_TARGET"
        fi
    else
        warn "rustup не найден — предполагаю, что цель $ANDROID_TARGET уже установлена"
    fi
}

SDK="$(find_sdk)" || die \
    "Android SDK не найден. Задайте ANDROID_SDK_ROOT (нужны platforms/android-* и build-tools)."

# Локальные build-tools кладём в PATH — так работают и SDK, и Termux.
if [[ -d "$SDK/build-tools" ]]; then
    BUILD_TOOLS="$(find "$SDK/build-tools" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)"
    [[ -n "$BUILD_TOOLS" ]] && export PATH="$BUILD_TOOLS:$PATH"
fi

if [[ -n "${ANDROID_JAR:-}" ]]; then
    SDK_JAR="$ANDROID_JAR"
else
    SDK_JAR="$(find "$SDK" -type f -name android.jar 2>/dev/null | sort -V | tail -1)"
fi
[[ -n "$SDK_JAR" && -f "$SDK_JAR" ]] || die \
    "не найден android.jar. Установите platforms;android-$TARGET_SDK или задайте ANDROID_JAR."

need javac "Установите JDK (например, openjdk-17)."
need d8 "Установите Android build-tools или пакет Termux."
need aapt2 "Установите Android build-tools."
need zip "Установите zip."
need unzip "Установите unzip."

NDK="$(find_ndk)" || NDK=""

if [[ -n "$NDK" ]]; then
    NDK_HOST="$(
        case "$(uname -s)-$(uname -m)" in
            Linux-x86_64)          echo linux-x86_64 ;;
            Linux-aarch64)         echo linux-x86_64 ;;
            Darwin-x86_64)         echo darwin-x86_64 ;;
            Darwin-arm64)          echo darwin-x86_64 ;;
            MINGW*|MSYS*|CYGWIN*)  echo windows-x86_64 ;;
            *)                     echo "" ;;
        esac
    )"
    NDK_PREBUILT="$NDK/toolchains/llvm/prebuilt/$NDK_HOST"
    if [[ ! -d "$NDK_PREBUILT" ]]; then
        NDK_PREBUILT="$(find "$NDK/toolchains/llvm/prebuilt" -mindepth 1 -maxdepth 1 -type d | head -1)"
    fi
    LINKER="$NDK_PREBUILT/bin/${CLANG_TRIPLE}${ANDROID_API}-clang"
    [[ -x "$LINKER" ]] || LINKER="$NDK_PREBUILT/bin/clang"
    [[ -x "$LINKER" ]] || die "не найден clang в NDK: $NDK_PREBUILT/bin"
    LIBCXX="$NDK_PREBUILT/sysroot/usr/lib/$LIBCXX_TRIPLE/libc++_shared.so"
    [[ -f "$LIBCXX" ]] || die "не найден libc++_shared.so в NDK ($LIBCXX)"
    MODE="ndk"
else
    [[ -n "${PREFIX:-}" ]] || die \
        "Android NDK не найден. Задайте ANDROID_NDK_HOME или запускайте сборку в Termux."
    LINKER="$PREFIX/bin/clang"
    LIBCXX="$PREFIX/lib/libc++_shared.so"
    [[ -x "$LINKER" ]] || die "не найден clang: $LINKER"
    [[ -f "$LIBCXX" ]] || die "не найден libc++_shared.so: $LIBCXX"
    MODE="termux"
fi

log "окружение: $MODE; SDK=$SDK"
log "APK: версия $VERSION, versionCode $VERSION_CODE, ABI $ABI, minSdk $ANDROID_API, targetSdk $TARGET_SDK"

TARGET_ENV="$(printf '%s' "$ANDROID_TARGET" | tr '[:lower:]-' '[:upper:]_')"
CXX="$LINKER"
if [[ "$LINKER" == *clang ]]; then
    CXX="${LINKER}++"
fi
AR_BIN="$(dirname "$LINKER")/llvm-ar"
[[ -x "$AR_BIN" ]] || AR_BIN="$(command -v llvm-ar || command -v ar || true)"
RANLIB_BIN="$(dirname "$LINKER")/llvm-ranlib"
[[ -x "$RANLIB_BIN" ]] || RANLIB_BIN="$(command -v llvm-ranlib || command -v ranlib || true)"

export "CARGO_TARGET_${TARGET_ENV}_LINKER=$LINKER"
export "CC_${ANDROID_TARGET//-/_}=$LINKER"
export "CXX_${ANDROID_TARGET//-/_}=$CXX"
if [[ -n "$AR_BIN" ]]; then
    export "AR_${ANDROID_TARGET//-/_}=$AR_BIN"
fi
if [[ -n "$RANLIB_BIN" ]]; then
    export "RANLIB_${ANDROID_TARGET//-/_}=$RANLIB_BIN"
fi

if [[ "$MODE" == "termux" ]]; then
    need patchelf "Установите patchelf: pkg install patchelf."
    # В Termux нет статической libc++ из NDK — подменяем системной.
    mkdir -p "$ROOT/target/android-cxx"
    ln -sf "$LIBCXX" "$ROOT/target/android-cxx/libc++_static.so"
    ln -sf "$LIBCXX" "$ROOT/target/android-cxx/libc++abi.so"
    RUSTFLAGS_VAR="CARGO_TARGET_${TARGET_ENV}_RUSTFLAGS"
    export "$RUSTFLAGS_VAR=${!RUSTFLAGS_VAR:-} -Clink-arg=-L$ROOT/target/android-cxx"
fi

ensure_rust_target

log "сборка libkumir_gui.so ($ANDROID_TARGET)"
cargo build -p kumir-gui --lib --release --target "$ANDROID_TARGET"

SO="$ROOT/target/$ANDROID_TARGET/release/libkumir_gui.so"
[[ -f "$SO" ]] || die "не найдена библиотека $SO"
if command -v patchelf >/dev/null 2>&1; then
    patchelf --remove-rpath "$SO"
fi

GAMES_AAR="$ANDROID_DIR/third_party/games-activity-$GAMES_VER.aar"
if [[ ! -f "$GAMES_AAR" ]]; then
    need curl "Установите curl или положите $GAMES_AAR вручную."
    log "скачиваю games-activity-$GAMES_VER.aar"
    mkdir -p "$(dirname "$GAMES_AAR")"
    curl -fL --retry 3 --connect-timeout 20 \
        -o "$GAMES_AAR.tmp" \
        "https://dl.google.com/dl/android/maven2/androidx/games/games-activity/$GAMES_VER/games-activity-$GAMES_VER.aar"
    mv "$GAMES_AAR.tmp" "$GAMES_AAR"
fi

log "подготовка Java/DEX"
rm -rf "$BUILD"
mkdir -p "$BUILD/lib/$ABI" "$BUILD/aar" "$BUILD/java"
unzip -o -j -q "$GAMES_AAR" classes.jar -d "$BUILD/aar"
unzip -o -q "$BUILD/aar/classes.jar" -d "$BUILD/aar-classes"

# Заменяем Google GameActivity своим классом (не требует appcompat).
rm -f "$BUILD"/aar-classes/com/google/androidgamesdk/GameActivity*.class
(cd "$BUILD/aar-classes" && zip -1 -rq ../gtinput.jar com)

find "$ANDROID_DIR/java-src" -name '*.java' -print >"$BUILD/java/sources.txt"
javac -Xlint:-options -source 11 -target 11 \
    -cp "$SDK_JAR:$BUILD/gtinput.jar" \
    -d "$BUILD/java" @"$BUILD/java/sources.txt"
(cd "$BUILD/java" && zip -1 -rq ../app.jar com androidx)

d8 --release --lib "$SDK_JAR" --min-api "$ANDROID_API" --output "$BUILD" \
    "$BUILD/gtinput.jar" "$BUILD/app.jar"
[[ -f "$BUILD/classes.dex" ]] || die "d8 не создал classes.dex"

log "сборка ресурсов и APK"
aapt2 compile --dir "$ANDROID_DIR/res" -o "$BUILD/res.zip"
aapt2 link -o "$BUILD/kumir-unsigned.apk" -I "$SDK_JAR" \
    --manifest "$ANDROID_DIR/AndroidManifest.xml" \
    --version-code "$VERSION_CODE" --version-name "$VERSION" \
    --min-sdk-version "$ANDROID_API" --target-sdk-version "$TARGET_SDK" \
    --auto-add-overlay "$BUILD/res.zip"

cp "$SO" "$BUILD/lib/$ABI/libkumir_gui.so"
cp "$LIBCXX" "$BUILD/lib/$ABI/libc++_shared.so"
(cd "$BUILD" && zip -1 -q -X kumir-unsigned.apk classes.dex \
    "lib/$ABI/libkumir_gui.so" "lib/$ABI/libc++_shared.so")

mkdir -p "$DIST/android"
if [[ "${ANDROID_UNSIGNED:-0}" == "1" ]]; then
    OUT="$DIST/android/kumir-ide-$VERSION-$ABI-unsigned.apk"
    cp "$BUILD/kumir-unsigned.apk" "$OUT"
    log "готово (без подписи): $OUT"
    exit 0
fi

need apksigner "Установите Android build-tools (apksigner)."
KEYSTORE="${ANDROID_KEYSTORE:-$ANDROID_DIR/kumir.keystore}"
KS_PASS="${ANDROID_KEYSTORE_PASS:-kumirpass}"
KEY_PASS="${ANDROID_KEY_PASS:-$KS_PASS}"
[[ -f "$KEYSTORE" ]] || die "не найден keystore: $KEYSTORE"

OUT_NAME="kumir-ide-$VERSION"
if [[ "$ABI" != "arm64-v8a" ]]; then
    OUT_NAME="$OUT_NAME-$ABI"
fi
OUT="$DIST/android/$OUT_NAME.apk"
SIGN_ARGS=(
    sign
    --ks "$KEYSTORE"
    --ks-pass "pass:$KS_PASS"
    --key-pass "pass:$KEY_PASS"
    --min-sdk-version "$ANDROID_API"
    --out "$OUT"
)
[[ -n "${ANDROID_KEY_ALIAS:-}" ]] && SIGN_ARGS+=(--ks-key-alias "$ANDROID_KEY_ALIAS")

log "подпись APK"
apksigner "${SIGN_ARGS[@]}" "$BUILD/kumir-unsigned.apk"
apksigner verify "$OUT"

log "готово: $OUT"
ls -lh "$OUT"
