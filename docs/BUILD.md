# Сборка КуМир на Rust

Все платформы собираются через один вход — `scripts/build.sh` (Linux/macOS/
Termux) или `scripts/build.ps1` (Windows). Makefile — короткая обёртка:

```bash
make help
make linux
make windows
make apk
```

Артефакты складываются в `dist/`:

```
dist/
├── linux/kumir-ide-<версия>-linux-<arch>.tar.gz
├── windows/kumir-ide-<версия>-windows-<arch>.zip
└── android/kumir-ide-<версия>[-<abi>].apk
```

Версия берётся из `[workspace.package] version` в `Cargo.toml`; отдельно её
менять в манифесте Android не нужно.

## Цели Rust

| Платформа | Цель Rust | Команда | Результат |
|---|---|---|---|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `make linux` | `dist/linux/*.tar.gz` |
| Linux arm64 | `aarch64-unknown-linux-gnu` | `make linux` на arm64 | `dist/linux/*.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-gnu` (кросс) или `x86_64-pc-windows-msvc` (нативно) | `make windows` | `dist/windows/*.zip` |
| Android arm64 | `aarch64-linux-android` | `make apk` | `dist/android/*.apk` |
| Android armv7 | `armv7-linux-androideabi` | `ANDROID_TARGET=... make apk` | `dist/android/*.apk` |

Установить дополнительные цели Rust можно заранее:

```bash
rustup target add x86_64-pc-windows-gnu
rustup target add aarch64-linux-android armv7-linux-androideabi
```

## Linux

Требуется Rust 1.95+ и системные библиотеки X11/Wayland/OpenGL (они
загружаются во время выполнения; для сборки обычно достаточно стандартного
набора):

```bash
# Arch/CachyOS
sudo pacman -S rust base-devel libxkbcommon wayland libglvnd

# Debian/Ubuntu
sudo apt install build-essential rustc cargo \
    libxkbcommon-dev libwayland-dev libx11-dev libgl1-mesa-dev
```

Сборка и упаковка:

```bash
make linux
```

В архиве:

```
kumir-ide-<версия>-linux-<arch>/
├── bin/kumir-ide
├── bin/kumir-run
├── share/applications/kumir-ide.desktop
├── share/icons/hicolor/scalable/apps/kumir-ide.svg
├── share/mime/packages/org.kumir.rust.xml
├── examples/
├── docs/
├── README.md
└── LICENSE.md
```

Локальная установка (по желанию):

```bash
tar -xzf dist/linux/kumir-ide-*.tar.gz -C /tmp
sudo install -m755 /tmp/kumir-ide-*/bin/* /usr/local/bin/
sudo install -Dm644 /tmp/kumir-ide-*/share/applications/kumir-ide.desktop \
    /usr/share/applications/kumir-ide.desktop
sudo install -Dm644 /tmp/kumir-ide-*/share/icons/hicolor/scalable/apps/kumir-ide.svg \
    /usr/share/icons/hicolor/scalable/apps/kumir-ide.svg
sudo install -Dm644 /tmp/kumir-ide-*/share/mime/packages/org.kumir.rust.xml \
    /usr/share/mime/packages/org.kumir.rust.xml
sudo update-mime-database /usr/share/mime
sudo gtk-update-icon-cache -f /usr/share/icons/hicolor || true
```

Запуск без установки:

```bash
./target/release/kumir-ide examples/hello.kum
./target/release/kumir-run examples/hello.kum
```

## Windows

### Нативно (MSVC)

1. Установите [Rust](https://rustup.rs) и «Desktop development with C++» из
   Visual Studio Build Tools.
2. В PowerShell:

```powershell
pwsh -File scripts/build.ps1 windows
```

Готовый архив: `dist\windows\kumir-ide-<версия>-windows-x86_64.zip`.

### Кросс-сборка из Linux/macOS (GNU)

Нужен `mingw-w64` и цель Rust:

```bash
# Arch/CachyOS
sudo pacman -S mingw-w64-gcc

# Debian/Ubuntu
sudo apt install gcc-mingw-w64-x86-64

rustup target add x86_64-pc-windows-gnu
make windows
```

По умолчанию используется `x86_64-pc-windows-gnu` и линкер
`x86_64-w64-mingw32-gcc`. Переопределить можно так:

```bash
WINDOWS_TARGET=x86_64-pc-windows-gnu \
WINDOWS_LINKER=/usr/bin/x86_64-w64-mingw32-gcc \
make windows
```

Для Windows-целей включена статическая C-библиотека CRT
(`target-feature=+crt-static`), поэтому в архив не нужно класть DLL рантайма
ни для GNU-, ни для MSVC-сборки.

## Android

Поддерживаются два варианта.

### Вариант 1: Android SDK + NDK (Linux/macOS/CI)

Установите:

* Android SDK: `platforms;android-34`, `build-tools;34.0.0`;
* Android NDK (r26+);
* JDK 17+;
* `zip`, `unzip`;
* цель Rust `aarch64-linux-android`.

```bash
export ANDROID_SDK_ROOT="$HOME/Android/Sdk"
export ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/27.2.12479018"

rustup target add aarch64-linux-android
make apk
```

Скрипт сам находит `android.jar`, `aapt2`, `d8`, `apksigner` и libc++
в NDK. Для эмулятора или 32-битных устройств:

```bash
ANDROID_TARGET=x86_64-linux-android make apk
ANDROID_TARGET=armv7-linux-androideabi make apk
```

### Вариант 2: Termux (arm64, без NDK)

```bash
pkg install rust clang make openjdk-17 aapt2 d8 apksigner zip unzip curl patchelf
make apk
```

По умолчанию SDK ищется в `$PREFIX/tmp/opencode/android/sdk` (совместимость
со старым скриптом), а также в `ANDROID_SDK_ROOT`, `ANDROID_HOME`,
`~/Android/Sdk` и `/opt/android-sdk`. Если `android.jar` лежит отдельно:

```bash
export ANDROID_JAR=/path/to/android.jar
make apk
```

### Подпись и версия

* `versionName` — версия из `Cargo.toml`;
* `versionCode` — `major*10000 + minor*100 + patch` (3.0.8 → 30008),
  переопределяется через `ANDROID_VERSION_CODE`;
* ключ по умолчанию — `android/kumir.keystore` с паролем `kumirpass`.

> **Важно.** Ключ `android/kumir.keystore` — учебный/отладочный. Для
> публикации замените его своим и не храните пароль в репозитории.

Своя подпись:

```bash
ANDROID_KEYSTORE=$HOME/release.keystore \
ANDROID_KEYSTORE_PASS='...' \
ANDROID_KEY_PASS='...' \
ANDROID_KEY_ALIAS=mykey \
make apk
```

Неподписанный APK (например, для подписи вручную):

```bash
ANDROID_UNSIGNED=1 make apk
```

Установка на устройство:

```bash
adb install -r dist/android/kumir-ide-*.apk
```

## Переменные окружения

| Переменная | Назначение | По умолчанию |
|---|---|---|
| `DIST` | каталог артефактов | `dist/` |
| `WINDOWS_TARGET` | цель Rust для Windows | `x86_64-pc-windows-gnu` при кросс-сборке, иначе host |
| `WINDOWS_LINKER` | mingw-w64 линкер | `x86_64-w64-mingw32-gcc` |
| `ANDROID_TARGET` | цель Rust для Android | `aarch64-linux-android` |
| `ANDROID_API` | minSdk | `24` |
| `ANDROID_TARGET_SDK` | targetSdk | `34` |
| `ANDROID_VERSION_CODE` | versionCode APK | вычисляется из версии |
| `ANDROID_SDK_ROOT` / `ANDROID_HOME` | путь к SDK | автоопределение |
| `ANDROID_NDK_HOME` / `ANDROID_NDK_ROOT` | путь к NDK | `$SDK/ndk/*` |
| `ANDROID_JAR` | явный путь к `android.jar` | поиск в SDK |
| `ANDROID_KEYSTORE` | ключ подписи | `android/kumir.keystore` |
| `ANDROID_KEYSTORE_PASS` | пароль хранилища | `kumirpass` |
| `ANDROID_KEY_PASS` | пароль ключа | равен паролю хранилища |
| `ANDROID_KEY_ALIAS` | alias ключа | первый ключ в хранилище |
| `ANDROID_UNSIGNED` | `1` — не подписывать APK | не задано |
| `ANDROID_GAMES_VERSION` | версия `games-activity` | `4.4.0` |

## Прочие команды

```bash
make test        # cargo test --workspace
make check       # cargo check --workspace --all-targets
make fmt         # форматирование
make fmt-check   # проверка форматирования (для CI)
make clippy      # clippy
make run         # запуск IDE
make cli         # kumir-run (аргументы: make cli ARGS="examples/hello.kum")
make doctor      # что установлено в системе
make clean       # удалить target/ и android/build/
make distclean   # clean + удалить dist/
```

## CI

`.github/workflows/build.yml` собирает артефакты для Linux, Windows и Android
на каждый push/PR и прикрепляет их к запуску. Для Android в CI используются
`android-actions/setup-android` и `nttld/setup-ndk`; подпись — тем же
отладочным ключом из репозитория (для настоящего релиза переопределите
переменные подписи через секреты).

## Если что-то не собирается

* `linker not found` для Windows — установите mingw-w64 или задайте
  `WINDOWS_LINKER`.
* `failed to find tool "aarch64-linux-android-clang++"` — не задан NDK.
  Установите `ANDROID_NDK_HOME` или запускайте сборку в Termux.
* `android.jar not found` — установите `platforms;android-34` или задайте
  `ANDROID_JAR`.
* `apksigner: command not found` — установите `build-tools` или пакет
  Termux `apksigner`.
* Ошибка `versionCode` при обновлении APK — увеличьте версию в
  `Cargo.toml` или задайте `ANDROID_VERSION_CODE` больше установленного.
