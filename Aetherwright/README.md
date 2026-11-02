# Aetherwright — First Playable

Оригинальная 3D voxel sandbox-игра об исследовании, инженерии и магии.
Rust + Bevy 0.20.1 + Avian 0.8 (физика).

## Требования

* Rust **1.97.1** (зафиксирован в `rust-toolchain.toml`; rustup установит его сам).
* macOS (Apple Silicon) — основная платформа. Linux и Windows тоже собираются.
* Linux: `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`.

## Сборка и запуск

```bash
cd Aetherwright
cargo run --release          # игра
cargo run --release -- --creative   # сразу в креативе
cargo test                   # юнит-тесты (мир, мешинг, инвентарь, сохранения…)
./run_autotest.sh [dir]      # сквозной сценарий в реальной игре + скриншоты
```

Отладочная сборка (`cargo run`) тоже играбельна: зависимости собираются
с `opt-level = 3`. Для быстрой пересборки: `cargo run --features dev`
(динамическая линковка Bevy).

## Управление

| Клавиша | Действие |
|---|---|
| WASD | движение |
| Shift | бег |
| Ctrl / C | присесть |
| Space | прыжок / всплыть |
| Мышь | обзор |
| V | 1-е / 3-е лицо |
| Alt + колесо | дистанция камеры (3-е лицо) |
| F (креатив) | полёт |
| Esc | пауза / освободить курсор |
| F3 | панель отладки |
| F4 | отладочная отрисовка физики |

Полный список управления — в разделе «Управление» файла `GAME_DESIGN.md`.

## Конфигурация

`assets/config/game.json` (необязателен) переопределяет любые поля
`GameConfig` (см. `src/core/config.rs`): размер чанка, размер мира,
seed, чувствительность мыши, FOV, дистанцию камеры, скорости персонажа…

## Структура

См. `DEVELOPMENT_LOG.md` (архитектура и ход работ) и `GAME_DESIGN.md`
(игровые решения).
