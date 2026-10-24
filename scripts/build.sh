#!/usr/bin/env bash
# Единая точка входа для сборки КуМир на Rust.
#
#   scripts/build.sh linux      # Linux: бинарники + tar.gz
#   scripts/build.sh windows    # Windows: .exe + zip (кросс-сборка из Linux/macOS)
#   scripts/build.sh apk        # Android: подписанный APK
#   scripts/build.sh doctor     # проверка окружения
#
# Подробности: docs/BUILD.md

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DIST="${DIST:-$ROOT/dist}"
[[ "$DIST" == /* ]] || DIST="$ROOT/$DIST"
VERSION="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
HOST_TARGET="$(rustc -vV 2>/dev/null | awk '/^host:/ { print $2 }' || true)"
HOST_TARGET="${HOST_TARGET:-unknown}"

log()  { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

need() {
    local tool="$1" hint="${2:-}"
    if ! command -v "$tool" >/dev/null 2>&1; then
        die "не найдена команда «$tool».${hint:+ $hint}"
    fi
}

host_arch() {
    case "$HOST_TARGET" in
        x86_64-*)          echo x86_64 ;;
        i686-*|i586-*)     echo x86 ;;
        aarch64-*)         echo aarch64 ;;
        armv7-*|arm-*)     echo armv7 ;;
        *)                 echo "${HOST_TARGET%%-*}" ;;
    esac
}

ensure_rust_target() {
    local target="$1" libdir
    [[ "$target" == "$HOST_TARGET" ]] && return 0

    # Иногда std уже лежит в toolchain, но rustup об этом не знает.
    libdir="$(rustc --print target-libdir --target "$target" 2>/dev/null || true)"
    if [[ -n "$libdir" && -d "$libdir" ]] && compgen -G "$libdir/libstd-*.rlib" >/dev/null; then
        return 0
    fi

    if command -v rustup >/dev/null 2>&1; then
        if ! rustup target list --installed | grep -qx "$target"; then
            log "устанавливаю цель Rust: $target"
            rustup target add "$target" || die "не удалось установить цель $target"
        fi
    else
        warn "rustup не найден — предполагаю, что цель $target уже установлена"
    fi
}

stage_common() {
    local stage="$1"
    cp README.md LICENSE.md "$stage/"
    mkdir -p "$stage/examples" "$stage/docs"
    cp examples/*.kum "$stage/examples/"
    find docs -maxdepth 1 -type f \( -name '*.md' -o -name '*.png' \) \
        -exec cp {} "$stage/docs/" \;
}

make_zip() {
    local dir="$1" out="$2"
    rm -f "$out"
    if command -v zip >/dev/null 2>&1; then
        (cd "$(dirname "$dir")" && zip -1 -rq -X "$out" "$(basename "$dir")")
    elif command -v 7z >/dev/null 2>&1; then
        (cd "$(dirname "$dir")" && 7z a -tzip -mx=1 "$out" "$(basename "$dir")" >/dev/null)
    elif command -v python3 >/dev/null 2>&1; then
        python3 - "$dir" "$out" <<'PY'
import os
import sys
import zipfile

src, dst = sys.argv[1], sys.argv[2]
base = os.path.dirname(src)
with zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED, compresslevel=1) as zf:
    for root, dirs, files in os.walk(src):
        dirs.sort()
        for name in sorted(files):
            path = os.path.join(root, name)
            zf.write(path, os.path.relpath(path, base))
PY
    else
        die "для упаковки нужен zip, 7z или python3"
    fi
}

build_linux() {
    need cargo "Установите Rust: https://rustup.rs"
    need tar
    local arch; arch="$(host_arch)"
    local stage="$DIST/linux/kumir-ide-$VERSION-linux-$arch"
    local archive="$DIST/linux/kumir-ide-$VERSION-linux-$arch.tar.gz"

    log "сборка Linux ($HOST_TARGET)"
    cargo build --release --bin kumir-ide --bin kumir-run

    log "упаковка $archive"
    rm -rf "$stage"
    mkdir -p "$stage/bin" \
        "$stage/share/applications" \
        "$stage/share/icons/hicolor/scalable/apps" \
        "$stage/share/mime/packages"

    cp target/release/kumir-ide target/release/kumir-run "$stage/bin/"
    cp packaging/linux/kumir-ide.desktop "$stage/share/applications/"
    cp packaging/linux/kumir-ide.svg "$stage/share/icons/hicolor/scalable/apps/"
    cp packaging/linux/org.kumir.rust.xml "$stage/share/mime/packages/"
    stage_common "$stage"

    tar -C "$(dirname "$stage")" -czf "$archive" "$(basename "$stage")"
    rm -rf "$stage"
    log "готово: $archive"
}

windows_arch() {
    case "$1" in
        x86_64-*)   echo x86_64 ;;
        i686-*)     echo x86 ;;
        aarch64-*)  echo aarch64 ;;
        *)          echo "${1%%-*}" ;;
    esac
}

build_windows() {
    need cargo "Установите Rust: https://rustup.rs"

    local target="${WINDOWS_TARGET:-}"
    if [[ -z "$target" ]]; then
        if [[ "$HOST_TARGET" == *-windows-* ]]; then
            target="$HOST_TARGET"
        else
            target="x86_64-pc-windows-gnu"
        fi
    fi

    local arch; arch="$(windows_arch "$target")"

    if [[ "$target" == *-msvc && "$HOST_TARGET" != *-windows-* ]]; then
        die "MSVC-цель $target нельзя собрать с $HOST_TARGET. Используйте Windows-хост или цель *-gnu с mingw-w64."
    fi

    if [[ "$target" == *-gnu ]]; then
        local linker="${WINDOWS_LINKER:-}"
        if [[ -z "$linker" ]]; then
            case "$target" in
                x86_64-*) linker="x86_64-w64-mingw32-gcc" ;;
                i686-*)   linker="i686-w64-mingw32-gcc" ;;
                *)        linker="${target%%-*}-w64-mingw32-gcc" ;;
            esac
        fi
        command -v "$linker" >/dev/null 2>&1 || die \
            "не найден mingw-w64 линкер «$linker». Установите mingw-w64 (Arch: mingw-w64-gcc, Debian/Ubuntu: gcc-mingw-w64-x86-64) или задайте WINDOWS_LINKER."
        local target_env
        target_env="$(printf '%s' "$target" | tr '[:lower:]-' '[:upper:]_')"
        export "CARGO_TARGET_${target_env}_LINKER=$linker"
    fi

    ensure_rust_target "$target"

    log "сборка Windows ($target)"
    cargo build --release --target "$target" --bin kumir-ide --bin kumir-run

    local stage="$DIST/windows/kumir-ide-$VERSION-windows-$arch"
    local archive="$DIST/windows/kumir-ide-$VERSION-windows-$arch.zip"
    rm -rf "$stage"
    mkdir -p "$stage/bin"
    cp "target/$target/release/kumir-ide.exe" \
       "target/$target/release/kumir-run.exe" "$stage/bin/"
    stage_common "$stage"

    log "упаковка $archive"
    make_zip "$stage" "$archive"
    rm -rf "$stage"
    log "готово: $archive"
}

build_android() {
    exec bash "$ROOT/scripts/build-android.sh" "$@"
}

run_tests()   { cargo test --workspace "$@"; }
run_check()   { cargo check --workspace --all-targets; }
run_fmt()     { cargo fmt --all; }
run_fmt_check() { cargo fmt --all -- --check; }
run_clippy()  { cargo clippy --workspace --all-targets; }

clean_build() {
    log "удаляю target/ и android/build/"
    rm -rf "$ROOT/target" "$ROOT/android/build"
}

doctor() {
    printf 'Версия проекта: %s\n' "$VERSION"
    printf 'Rust:           '; rustc -V 2>/dev/null || echo "не найден"
    printf 'Cargo:          '; cargo -V 2>/dev/null || echo "не найден"
    printf 'Host target:    %s\n' "$HOST_TARGET"
    if command -v rustup >/dev/null 2>&1; then
        printf 'Rustup:         '; rustup show active-toolchain 2>/dev/null || true
        printf 'Установлены:    '; rustup target list --installed | tr '\n' ' '; echo
    else
        printf 'Rustup:         не найден\n'
    fi
    echo
    echo "Инструменты:"
    local tools=(zip 7z python3 tar curl javac keytool aapt2 d8 apksigner patchelf x86_64-w64-mingw32-gcc)
    local tool
    for tool in "${tools[@]}"; do
        if command -v "$tool" >/dev/null 2>&1; then
            printf '  %-28s %s\n' "$tool" "$(command -v "$tool")"
        else
            printf '  %-28s %s\n' "$tool" "—"
        fi
    done
}

usage() {
    cat <<EOF
КуМир на Rust — сборка $VERSION

Использование:
  scripts/build.sh <команда> [аргументы]
  make <цель>

Команды:
  linux        собрать Linux-бинарники и tar.gz в dist/linux/
  windows      собрать Windows-бинарники и zip в dist/windows/
  apk          собрать подписанный APK в dist/android/
               (то же самое: android)
  test         cargo test --workspace
  check        cargo check --workspace --all-targets
  fmt          отформатировать код
  fmt-check    проверить форматирование
  clippy       запустить clippy
  run          запустить IDE (аргументы передаются kumir-ide)
  cli          запустить kumir-run (аргументы передаются программе)
  clean        удалить target/ и android/build/
  distclean    clean + удалить dist/
  doctor       показать версии Rust и доступные инструменты
  help         эта справка

Переменные окружения:
  DIST                 каталог артефактов (по умолчанию dist/)
  WINDOWS_TARGET       цель Rust для Windows (по умолчанию x86_64-pc-windows-gnu)
  WINDOWS_LINKER       mingw-w64 линкер
  ANDROID_TARGET       цель Rust для Android (по умолчанию aarch64-linux-android)
  ANDROID_SDK_ROOT     путь к Android SDK
  ANDROID_NDK_HOME     путь к Android NDK
  ANDROID_VERSION_CODE versionCode APK (по умолчанию вычисляется из версии)
  ANDROID_KEYSTORE     путь к keystore
  ANDROID_KEYSTORE_PASS, ANDROID_KEY_PASS, ANDROID_KEY_ALIAS
  ANDROID_UNSIGNED=1   не подписывать APK

Подробно: docs/BUILD.md
EOF
}

cmd="${1:-help}"
[[ $# -gt 0 ]] && shift

case "$cmd" in
    help|-h|--help) usage ;;
    linux)          build_linux ;;
    windows)        build_windows ;;
    apk|android)    build_android "$@" ;;
    test)           run_tests "$@" ;;
    check)          run_check ;;
    fmt)            run_fmt ;;
    fmt-check)      run_fmt_check ;;
    clippy)         run_clippy ;;
    run)            cargo run -p kumir-gui -- "$@" ;;
    cli)            cargo run -p kumir-cli -- "$@" ;;
    clean)          clean_build ;;
    distclean)      clean_build; log "удаляю $DIST"; rm -rf "$DIST" ;;
    doctor)         doctor ;;
    *)              die "неизвестная команда «$cmd». Запустите scripts/build.sh help" ;;
esac
