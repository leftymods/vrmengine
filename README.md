# VRM Engine

Порт [@pixiv/three-vrm](https://github.com/pixiv/three-vrm) на Rust + OpenGL.
Загружает VRM-модели (`.vrm` / `.glb`), показывает их в окне, позволяет крутить выражения и следит за FPS — всё с цветным логом и отловом нативных крашей.

## Возможности

- **Загрузка VRM** — полный парсер glTF 2.0 + VRM-расширения: `meta`, `humanoid`, `expressions`, `lookAt`, `firstPerson`, `springBone`.
- **Просмотр** — окно на `winit` + `glutin`, рендер через `glow` (OpenGL), статичная сцена модели.
- **UI на `egui`** — боковая панель:
  - путь к файлу + кнопка **Browse...** (диалог с фильтром `.vrm` / `.glb`) и кнопка **Load VRM**;
  - поддержка **drag & drop** файла прямо в окно;
  - метаданные модели (название, версия, авторы);
  - **слайдеры выражений** (0.0–1.0) и кнопка **Reset All**;
  - кнопка **Quit**.
- **Логирование** — уровни `DEBUG / INFO / WARN / ERROR` в консоль (цвета только у токена `[LEVEL]`) и в файл `vrmengine.log`. На Windows цвета работают через консольный API, без ANSI-мусора.
- **Отлов крашей (Windows)** — обработчик SEH: если процесс упал (access violation, стек, `swap_buffers` и т.п.), в консоль и лог пишется красная строка `[ERROR] CRASH: <код> at <адрес>`, затем процесс завершается как обычно.

## Сборка и запуск

### Linux

```sh
cargo run --release
```

### Windows (cross-компиляция с Linux)

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu
```

Локальный конфиг линкера (`.cargo/config.toml`) в репозиторий не коммитится — положи его сам:

```toml
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
```

Готовый `.exe` — `target/x86_64-pc-windows-gnu/release/vrmengine.exe`.

## Использование

1. Запусти `vrmengine.exe` (или `cargo run --release`).
2. Перетащи `.vrm` файл в окно, или впиши путь и нажми **Browse...** → **Load VRM**.
3. Крути слайдеры выражений, жми **Reset All**, выходи через **Quit**.

Если модель не загрузилась — причина появится красным в панели и в логе.

## Структура проекта

```
src/
├── math.rs          # векторная/матричная математика (glam)
├── scene.rs         # узлы сцены, трансформы
├── gltf_loader.rs   # базовый glTF 2.0 парсер
├── material.rs      # материалы, PBR
├── vrm/             # VRM-расширения
│   ├── loader.rs    # общий загрузчик VRM
│   ├── model.rs     # высокоуровневая модель
│   ├── meta.rs      # метаданные
│   ├── humanoid.rs  # скелет-ориентация
│   ├── expression.rs# выражения (морф + материалы)
│   ├── lookat.rs    # взгляд
│   ├── firstperson.rs
│   └── springbone.rs
├── animation/       # анимация (через VRM-выражения)
├── renderer.rs      # отрисовка сцены (glow / OpenGL)
├── viewer.rs        # окно, egui-интерфейс, главный цикл
├── log.rs           # цветное логирование в консоль + файл
└── crash.rs         # Windows SEH-краш-хендлер
```

## Кредиты

Порт [three-vrm](https://github.com/pixiv/three-vrm) от pixiv (MIT). Инструменты: Rust, `winit`, `glutin`, `glow`, `egui`, `rfd`, `glam`, `image`, `serde`.
