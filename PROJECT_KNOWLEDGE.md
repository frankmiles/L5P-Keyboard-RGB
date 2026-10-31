# Legion Keyboard RGB — Project Knowledge Reference

For a detailed, self-contained product and implementation specification, see [`technical-spec-detail.md`](./technical-spec-detail.md). This GitHub-flavored Markdown page covers the workspace architecture, device contract, effects, input/audio, profiles, GUI/CLI, helper lifecycle, limitations, and build setup.

> **Purpose:** A self-contained, searchable reference for AI agents and maintainers working on this repository. It summarizes architecture, runtime behavior, device constraints, effect extension points, input handling, configuration, build workflows, and validation.
>
> **How to use it:** Start with the index below, jump to the relevant section, and verify implementation details against the linked source files before making changes. Source code is authoritative when this reference and the current checkout differ.
>
> **Repository:** `frankmiles/L5P-Keyboard-RGB`  
> **Project:** Legion Keyboard RGB Control  
> **Reference last reviewed:** 2026-11-01

## Search index

| If the task concerns... | Start here | Source of truth |
|---|---|---|
| Workspace, crate boundaries, application startup | [Architecture](#architecture) | `Cargo.toml`, `app/src/main.rs` |
| Keyboard protocols, devices, zones, speed, brightness | [Keyboard driver and hardware constraints](#keyboard-driver-and-hardware-constraints) | `driver/src/lib.rs` |
| Effect names, colors, direction, speed, CLI options | [Effects and profiles](#effects-and-profiles) | `app/src/enums.rs`, `app/src/manager/profile.rs`, `app/src/cli.rs` |
| Effect implementation or dispatch | [Adding or modifying a built-in effect](#adding-or-modifying-a-built-in-effect) | `app/src/manager/effects/`, `app/src/manager/mod.rs` |
| Stop signals, animation loops, application shutdown | [Manager lifecycle and cancellation](#manager-lifecycle-and-cancellation) | `app/src/manager/mod.rs` |
| Global typing, key events, key-zone layout | [Keyboard input](#keyboard-input) | `input/src/lib.rs`, `input/src/event.rs`, `app/src/manager/effects/zones.rs` |
| User-defined animated effects | [Custom effect JSON](#custom-effect-json) | `app/src/manager/custom_effect.rs`, `app/src/manager/mod.rs` |
| GUI settings, brightness, speed, direction | [GUI behavior](#gui-behavior) | `app/src/gui/mod.rs`, `app/src/manager/effects/mod.rs` |
| Build, tests, Windows dependencies, release artifact | [Build and validation](#build-and-validation) | `README.md`, `Makefile.toml`, `.github/workflows/release-rust.yml` |
| Persistent settings and profile files | [Persistence and paths](#persistence-and-paths) | `app/src/util.rs`, `app/src/manager/profile.rs`, `app/src/gui/` |
| Project-specific pitfalls and safe change checklist | [Engineering guidance](#engineering-guidance) | See relevant sections above |

## Project summary

This Rust desktop application controls the four individually addressable RGB keyboard zones on supported Lenovo Legion laptops. It combines:

1. A GUI for selecting effects, configuring colors and speed, loading profiles, and opening custom effects.
2. A CLI for listing/selecting effects, applying profiles, and loading custom-effect JSON.
3. A Rust HID driver that translates lighting state into keyboard-device reports.
4. A separate input crate that observes global key-down/key-up events for reactive effects.

The keyboard’s native firmware implements only a small set of basic animations. Effects beyond those firmware modes are software animations: the app repeatedly sends updated four-zone color arrays while the device is configured in static mode. Custom JSON effects use the same static-color update mechanism.

## Architecture

### Workspace and crate responsibilities

The root `Cargo.toml` defines a Cargo workspace with resolver version 2:

- **`app/` — `legion-kb-rgb`:** The GUI, CLI, profile/settings management, effect manager, custom-effect runner, and screen/temperature/input-driven effects.
- **`driver/` — `legion-rgb-driver`:** Supported-device discovery and HID operations. Maintains keyboard lighting state, constructs reports, and performs color transitions.
- **`input/` — `legion-input`:** Cross-platform global keyboard observation, normalized key values, event subscriptions, and pressed-key tracking.

The GUI is based on `eframe`/`egui`. CLI argument handling is implemented with `clap`. `serde`/`serde_json` handle serialized profiles and custom effects. The app already depends on `rand` for randomized visual effects.

### Application entry and control flow

`app/src/main.rs` dispatches CLI arguments through `cli::try_cli()`. With no CLI-only command, it starts the `eframe` window titled **Legion RGB**. On Windows the program has a GUI subsystem; panic reporting temporarily attaches a console when possible.

`app/src/manager/mod.rs` owns `EffectManager` and its worker thread:

1. Manager creation obtains the keyboard driver and optionally creates a global input listener. Failure to acquire input is intentionally represented as `None`; effects that need typing input must degrade gracefully.
2. The worker consumes `Message::Profile`, `Message::CustomEffect`, and `Message::Exit`.
3. Applying a profile configures the driver's brightness and speed before dispatching to a built-in effect.
4. The GUI worker coalesces queued messages by taking the most recent profile. CLI mode consumes one message at a time.
5. On shutdown the manager signals active work to stop, sends `Exit`, and joins its worker thread.

Single-instance ownership is held for the lifetime of the manager. Do not remove the `SingleInstance` field from `Inner`: the code deliberately retains it to keep the process-wide reservation alive. The GUI has a separate instance guard so a second window is rejected even when the keyboard manager belongs to a background helper.

On Windows, clicking **Apply until shutdown** starts the same executable in a hidden background-effect mode. That process owns the `EffectManager`, so software animations, typing input, and optional WASAPI capture continue after the GUI exits. The helper is started only by the explicit button, has no service installation or startup entry, and ends when the user replaces a background effect with an app-session custom effect or Windows shuts down. When the GUI is reopened, it attaches to the running helper and forwards changed built-in profiles. It does not start a helper automatically after reboot.

The helper control channel is a local Windows named pipe, rejects remote clients, reserves its first pipe instance to detect name collisions, and accepts only bounded, typed JSON commands for status, profile updates, or stop. Profile names are omitted from helper startup arguments and updates. The helper accepts no shell commands, network connections, or automatic-start requests. Custom JSON effects are still supported in the GUI; choosing one while a helper is active stops the helper and resumes the normal in-process manager, so custom effects retain their existing app-session lifetime. The Apply button is Windows-only and is disabled while a custom effect is selected.

### Important directories

- `app/src/gui/` — application window, styling, tray interaction, and persisted GUI state.
- `app/src/manager/` — effect manager, profiles, custom effects, and effect implementations.
- `app/src/manager/effects/` — individual effects and shared effect UI.
- `driver/src/` — HID device support, report encoding, and error types.
- `input/src/` — portable input API plus platform-specific event capture.
- `.github/workflows/` — GitHub Actions build and release workflows.
- `output-app/` — local requested executable artifact directory; Cargo build output itself goes under ignored `target/`.

## Keyboard driver and hardware constraints

### Device support

`driver/src/lib.rs` recognizes specific Lenovo/ITE vendor/product/usage tuples. The current supported identifiers cover listed 2020–2024 Legion and Ideapad/LOQ models. Device acquisition is performed through HIDAPI; it does not search for an arbitrary RGB keyboard.

If a model cannot be found, inspect `KNOWN_DEVICE_INFOS`, the operating-system-specific matching logic, and `get_keyboard()` in `driver/src/lib.rs`. Do not assume a product ID alone is sufficient on Windows: the Windows match also includes usage page and usage.

### Four zones and color arrays

The driver controls four RGB zones. The canonical flat array contains exactly twelve channel values in this order:

```text
[zone0_r, zone0_g, zone0_b,
 zone1_r, zone1_g, zone1_b,
 zone2_r, zone2_g, zone2_b,
 zone3_r, zone3_g, zone3_b]
```

In compact form: `[r,g,b, r,g,b, r,g,b, r,g,b]`. Each channel is an unsigned 8-bit value from 0 to 255. `Profile::rgb_array()` maps disabled profile zones to black; `profile::arr_to_zones()` converts an array to four enabled zones.

Use the driver API rather than inventing a report format:

- `keyboard.set_colors_to(&[u8; 12])`
- `keyboard.transition_colors_to(&[u8; 12], steps, delay_between_steps)`
- `keyboard.set_zone_by_index(zone_index, [r, g, b])`
- `keyboard.set_effect(BaseEffects::...)`
- `keyboard.set_speed(speed)`
- `keyboard.set_brightness(brightness)`

Zone indices are 0–3. The driver validates the index, brightness, and speed. `SPEED_RANGE` is `1..=4`, `BRIGHTNESS_RANGE` is `1..=2`, and `ZONE_RANGE` is `0..=3`.

### Firmware modes versus software effects

`BaseEffects` in the driver exposes only `Static`, `Breath`, `Smooth`, `LeftWave`, and `RightWave`.

`set_colors_to()` only updates the stored RGB channels while the firmware mode is `Static` or `Breath`. Software-driven effects must first configure `BaseEffects::Static`; otherwise color updates may be ignored. Firmware effects should use the existing direct dispatch in `Inner::apply_effect()` rather than a software animation loop.

The driver’s transition operation also observes a shared atomic stop signal between sub-steps. This is how interrupted profile/custom-effect transitions yield to newer work.

## Effects and profiles

### Effect enum and UI/CLI integration

`app/src/enums.rs` defines the `Effects` enum and its UI/CLI capability helpers:

- `takes_color_array()` controls whether per-zone colors are meaningful/editable and whether CLI selection needs colors.
- `takes_direction()` enables the direction control.
- `takes_speed()` enables the speed slider.
- `is_built_in()` distinguishes the built-in speed range from the custom-effect range and tells the manager whether the keyboard firmware should receive built-in speed configuration.

`Effects` derives `Display`, `EnumString`, `EnumIter`, and Serde traits. These derives power the GUI selector, CLI parsing/listing, and serialized profiles. When adding an enum variant, update all applicable helper functions and ensure exhaustive matches compile.

The GUI options are shown by `app/src/manager/effects/mod.rs`; common brightness/direction/speed controls live in `default_ui.rs`. The GUI effect list is generated by iterating `Effects`, and CLI `list` uses the same enum, so a variant that is correctly derived and dispatched automatically appears in both lists. The color panel's Random button assigns four different vivid hues, spaced 90 degrees apart, to the four zone palette colors.

The bottom **Apply until shutdown** action is implemented in `app/src/gui/mod.rs` and `app/src/background.rs`. It starts the hidden Windows helper with the selected built-in profile, or updates a helper already running. GUI startup detects and reconnects to an active helper without opening a second keyboard manager. No task, service, registry run key, or persistent auto-start setting is created; Windows terminates the helper during shutdown.

### Current built-in effects

The firmware-supported effects are **Static**, **Breath**, **Smooth**, and **Wave**. Wave selects the firmware's left/right direction.

Other existing effects are software-driven:

- **Lightning** — randomized brief colored zone flashes.
- **AmbientLight** — captures the screen and maps sampled color to four zones; has FPS and saturation controls.
- **SmoothWave** and **Swipe** — software color transitions; selectable direction and swipe mode (`Change` or `Fill`), with optional black cleanup in Fill mode.
- **Disco** — randomized zone colors.
- **Christmas** — cycles festive color/pattern sub-effects.
- **Fade** — dims after inactivity and restores lighting when typing resumes.
- **Temperature** — displays a CPU-temperature gradient; platform support is limited.
- **Ripple** — uses keyboard input and mapped key zones to propagate color outward.

Twenty additional software-driven built-ins are implemented in `app/src/manager/effects/expanded.rs` and dispatched from `app/src/manager/mod.rs`:

| Effect | Visual behavior | Color/input behavior |
|---|---|---|
| **Spectrum** | Smooth HSV rainbow rotating across all four zones | Generated rainbow colors |
| **MatrixRain** | Random green/teal heads and fading trails | Generated colors; direction and speed influence movement |
| **ReactiveTyping** | Zone lights brighten as mapped keys are pressed, then decay | Uses profile zone colors; when input is unavailable, shows a subtle idle glow |
| **Pulse** | Repeating double-beat brightness envelope | Uses profile zone colors |
| **Aurora** | Slowly drifting, softly varying green/teal gradient | Generated colors |
| **SnakeTrail** | Bright moving head with an exponentially fading tail | Uses profile zone colors |
| **RadarSweep** | Moving radar-like beam over dim blue background | Uses profile zone colors |
| **Fire** | Randomized warm red/orange/yellow flicker and sparks | Generated colors |
| **Starfield** | Twinkling points with occasional short shooting-star trails | Generated cool cyan/blue colors |
| **Marquee** | Moving bright band with a colored trailing field | Generated HSV colors; direction changes travel |
| **BouncingBall** | Moving, bouncing point of light and local glow | Uses profile zone colors |
| **LaserBeamSweep** | Narrow, bright beam sweeping across zones | Uses profile zone colors |
| **DualColorSplit** | Contrasting hues across the keyboard with blended center zones | Generated magenta/cyan-like palette |
| **NeonSineWave** | Alternating neon hues with a sine-modulated brightness | Generated colors; direction changes phase travel |
| **RippleBurst** | Expanding wave, with its origin changing between zones | Uses profile zone colors; direction changes origin order |
| **StrobeFlash** | Alternating, high-contrast zone flashes | Uses profile zone colors |
| **TypingBloom** | Bright per-zone bloom fades after mapped keys are pressed | Uses the profile palette and key-zone mapping; idle glow if input is unavailable |
| **TypingWave** | Key presses launch a decaying wave into adjacent zones | Uses profile zone colors; gracefully idles if input is unavailable |
| **AudioVisualizer** | Fifteen selectable visualizer styles driven by analyzed frequency bands and beat transients | Windows system-output loopback or default-microphone capture; samples processed in memory only |

The **TypingStudio** built-in offers 15 distinct styles through its style selector: NeonRain, Heartbeat, Prism, Meteor, Ember, Ripple, Comet, GhostTrail, ZoneBounce, Starlight, Laser, RainbowSpark, Cascade, Aurora, and Staccato.

**AudioVisualizer** offers 15 distinct styles through its style selector: VuMeter, BeatPulse, Waveform, BassDrop, SpectrumBars, VocalGlow, TrebleSpark, BeatStrobe, Equalizer, SoundRipple, DanceFloor, Aurora, Siren, FireBeat, and CometTrail. On Windows, its dedicated WASAPI worker captures short system-output loopback or microphone frames in memory, calculates bass/mid/treble/voice-band levels, and publishes only those features. Audio samples are neither recorded nor persisted. Beat styles react to bass transients rather than treating a continuously loud signal as a beat; an inactive stream decays to zero. The audio worker is joined when the effect is replaced or the app shuts down. If no supported endpoint is available, the app prints a diagnostic and keeps the selected audio effect dark. Other platforms currently report unsupported audio capture and keep the effect dark.

### Profile shape and speed handling

`Profile` in `app/src/manager/profile.rs` is serialized and contains an optional name, four `KeyboardZone` values, effect, direction, speed, and brightness. Defaults are the `Static` effect, left direction, speed 1, and low brightness. Profile RGB conversion respects each zone’s `enabled` flag.

The manager clamps built-in speeds to the driver's `SPEED_RANGE` before calling `set_speed()`. Software animation timing should use a clamped speed, and any timing derived from speed must avoid a zero divisor. Keep the driver's limits as the single source of truth; do not broaden them to accommodate a visual effect.

Existing special cases to preserve:

- `SmoothWave` replaces the working profile colors with a preset four-color arrangement before invoking the shared swipe engine.
- `AmbientLight` clamps its own FPS to 1–60 and saturation boost to 0.0–1.0.
- Effects with no configurable palette may generate their own colors; do not unnecessarily change the shared profile colors for such effects.

## Adding or modifying a built-in effect

Use the established pattern and verify every user-facing surface:

1. **Name and serialization:** Add a unit variant to `Effects` in `app/src/enums.rs`. Keep the display/string names intentional; these names are visible in the GUI, CLI, and saved profiles.
2. **Controls:** Set `takes_color_array()`, `takes_direction()`, `takes_speed()`, and `is_built_in()` correctly. These helpers affect multiple surfaces, not just decoration.
3. **Implementation:** Add the effect under `app/src/manager/effects/` or group related implementations in a cohesive module. Put modules in `effects/mod.rs`.
4. **Dispatch:** Add an exhaustive arm to `Inner::apply_effect()` in `app/src/manager/mod.rs`.
5. **Driver setup:** For a software effect that calls `set_colors_to()` or transitions, set `BaseEffects::Static` first.
6. **Frame shape:** Always construct/send the 12-byte, four-zone RGB array in zone order.
7. **Timing and cancellation:** Poll `manager.stop_signals.manager_stop_signal` in the frame loop and keep sleeps short enough that a newly selected profile or shutdown is promptly observed. Use the existing driver transition API when smooth step-by-step transitions are needed.
8. **Input:** Use `Inner.input` if available. Do not make an effect unusable just because global input capture is unavailable.
9. **UI:** Add a custom UI arm in `effects/mod.rs` only if the effect needs controls beyond common brightness, direction, speed, or palette controls.
10. **Tests/docs:** Add deterministic unit tests for math/mapping helpers where practical, and update the README plus this reference when behavior changes.

Prefer deterministic frame calculations and small testable helpers. Use `rand` only for intentional stochastic visuals. Do not spawn a permanent unjoined thread per animation. If an effect starts a helper thread to listen for input, signal it to exit and join it on every normal stop path.

## Manager lifecycle and cancellation

`StopSignals` in `app/src/manager/mod.rs` contains two `Arc<AtomicBool>` signals:

- **`manager_stop_signal`:** Stops the current manager effect/custom-effect loop when a new message or shutdown arrives.
- **`keyboard_stop_signal`:** Interrupts in-progress driver transitions and can be used by existing input-aware loops.

`store_true()` sets both. `store_false()` clears both. `set_profile()`, `custom_effect()`, and `shutdown()` request a stop before enqueueing their message. `set_profile()` clears the signals before starting the new effect. Respect this ownership and state transition; do not reset the manager signal midway through an animation, or shutdown/replacement requests may be lost.

Animation functions execute synchronously on the manager worker. A loop that ignores the manager signal blocks later profiles and prevents the worker from processing `Exit`. Keep each frame cancellable. Existing helpers unwrap driver errors consistently with surrounding effect modules; do not hide errors behind silent defaults.

For input-thread effects such as `Fade` or `Ripple`, follow the established cleanup pattern in those files: use an atomic thread-exit flag, stop the worker loop, and join its handle before returning. Keep manager cancellation separate from an ordinary key-down event unless the intended interaction explicitly interrupts a transition.

## Keyboard input

`legion-input` starts a platform capture thread plus a dispatcher. Cloned `Input` handles share the same held-key set and event subscriptions. `Input::new()` can fail; the app turns this into `Option<Input>`.

The public API includes:

- `input.subscribe()` — receive future normalized `Event`s.
- `input.pressed_keys()`, `is_pressed()`, `any_pressed()` — inspect held key state.
- `EventKind::KeyDown(Key)`, `KeyUp(Key)`, and `Reset` — key press/release and state-clear notifications.

The normalized `Key` type describes physical key positions, not translated text characters. `RawCode` retains platform-specific details if a task genuinely needs the original OS keycode.

`app/src/manager/effects/zones.rs` assigns physical keys to four lighting zones through `KEY_ZONES`. Effects such as Ripple and ReactiveTyping use this shared mapping. Reuse it rather than introducing a second inconsistent mapping. Treat `Reset` as clearing all held-key state.

The GUI separately observes input for its profile-cycling shortcut. Effects should create their own subscriptions as needed; subscriptions receive future events, so also consider current held-key state when initializing a reactive effect.

## Custom effect JSON

Custom effects are a separate execution path from `Effects` built-ins. They are defined by `CustomEffect` in `app/src/manager/custom_effect.rs` and loaded from a JSON file using the existing storage helper.

Current schema:

```json
{
  "name": "MyEffect",
  "effect_steps": [
    {
      "rgb_array": [0, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
      "step_type": "Set",
      "brightness": 1,
      "steps": 50,
      "delay_between_steps": 20,
      "sleep": 100
    },
    {
      "rgb_array": [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 0, 255],
      "step_type": "Transition",
      "brightness": 2,
      "steps": 80,
      "delay_between_steps": 15,
      "sleep": 120
    }
  ],
  "should_loop": true
}
```

Field meanings:

- `name`: optional label.
- `effect_steps`: ordered animation frames.
- `rgb_array`: exactly 12 channel values in the four-zone order defined above.
- `step_type`: `"Set"` immediately applies colors; `"Transition"` interpolates toward them.
- `brightness`: driver brightness value, currently 1 (low) or 2 (high).
- `steps`: transition step count (`u8`).
- `delay_between_steps`: milliseconds between transition frames (`u64`).
- `sleep`: milliseconds to wait after a step.
- `should_loop`: repeat the frame sequence after the last step.

The custom-effect runner sets brightness per step and calls `set_colors_to()` or `transition_colors_to()` accordingly. Do not confuse `EffectStep.brightness` with the profile `Brightness` enum, which is converted to the driver's 1–2 range.

## GUI behavior

`app/src/gui/` owns the window and GUI state. `show_effect_ui()` provides the effect options panel. Common controls are brightness, direction, and speed; the color editor is gated by `takes_color_array()`. Custom settings can extend the match in `app/src/manager/effects/mod.rs`.

The available-effect selector is generated from `Effects::iter()`. The GUI sends profile changes to the effect manager and stops a currently loaded custom effect when a new built-in is selected. Custom-effect file management and loaded-effect state are separate from the profile's `Effects` enum. AudioVisualizer adds source and style selectors, with system-output and microphone input options on Windows. TypingStudio offers a selector for 15 typing animations. The color-panel Random button sets four distinct high-saturation zone colors spaced evenly around a randomly chosen hue circle; the global picker preview follows zone 1.

The application also has a system tray on supported operating systems. Closing the window may hide it to the tray rather than terminate the process; this depends on tray availability and platform rules. Linux under Wayland may prohibit hiding.

## CLI behavior

The CLI is implemented in `app/src/cli.rs`:

- `--gui` starts the GUI, optionally initialized with a profile or custom effect.
- `--hide-window` requests a hidden initial GUI window where supported.
- `set` applies one of the `Effects` variants with optional color array, brightness, speed, direction, and a path to save the profile.
- `list` iterates the effect enum and prints available effects.
- `load-profile --path <path>` loads a serialized profile.
- `custom-effect --path <path>` loads a custom JSON effect.

The color argument uses exactly twelve comma-separated `u8` values. Color parsing rejects invalid channels or an array of the wrong length. Effect helper methods determine whether colors/direction/speed apply. `Brightness` is `Low` or `High`; driver brightness numeric values are 1 and 2. CLI input is parsed by Clap; use `app --help` and `app set --help` for the live syntax.

## Persistence and paths

`Profile` and `CustomEffect` use Serde and the shared JSON `StorageTrait` in `app/src/util.rs`. The trait loads/saves JSON at the specified path. Profiles with no name are named `Untitled` when saved.

GUI settings and saved profiles/custom effects are managed in `app/src/gui/` and are not the same as the stand-alone profile file argument to CLI `load-profile`. Configuration defaults to the application's working directory unless overridden by `LEGION_KEYBOARD_CONFIG`. Avoid editing or committing generated user settings while changing source.

## Build and validation

### Rust workspace

From the repository root:

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked
cargo build --release --locked
```

`cargo build --release` produces `target/release/legion-kb-rgb.exe` on Windows and `target/release/legion-kb-rgb` on Linux. To create the requested local Windows deliverable:

```powershell
New-Item -ItemType Directory -Force output-app | Out-Null
Copy-Item target/release/legion-kb-rgb.exe output-app/app.exe -Force
```

Run tests/builds against the narrowest relevant crate when appropriate, then perform a workspace/release build for changes that affect shared interfaces or user-facing effects.

### Windows prerequisites

The Windows app build uses the **MSVC** Rust target and therefore requires Visual Studio C++ build tools, Windows SDK, and `link.exe`. The AudioVisualizer uses the `wasapi` crate to capture and analyze short Windows endpoint audio buffers in memory (system output loopback or microphone input) and publishes only derived frequency-band levels; it does not record or store the samples. The screen-capture dependency `scrap` generates C bindings using `bindgen`, which requires a compatible LLVM `libclang.dll`; set `LIBCLANG_PATH` to the directory containing it when it is not discoverable automatically.

The Windows build also links native screen/video libraries from vcpkg. The CI workflow pins vcpkg commit `9e593bb18ea69cc5095e012465dcd675a822ed0d` and uses the `x64-windows-static` triplet. The repository's `vcpkg.json` declares `libvpx`, `libyuv`, and `aom`. Configure `VCPKG_ROOT`, `VCPKG_INSTALLED_ROOT` (when using an explicit installed directory), and `VCPKG_DEFAULT_TRIPLET` before building. Use the pinned baseline/CI setup rather than an arbitrary system vcpkg version.

Typical Windows release-build sequence (after installing prerequisites and the three vcpkg libraries):

```powershell
$env:VCPKG_ROOT = "C:\path\to\vcpkg"
$env:VCPKG_INSTALLED_ROOT = "C:\path\to\vcpkg\installed"
$env:VCPKG_DEFAULT_TRIPLET = "x64-windows-static"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
cargo test -p legion-kb-rgb --locked
cargo build --release --locked
```

Run Cargo from an MSVC developer environment so the linker and SDK paths are configured. The repository documents cargo-make helpers in `Makefile.toml`; inspect their prerequisite/setup tasks before relying on them for a clean machine.

### Linux prerequisites and CI

Linux builds need the system development libraries listed in `.github/workflows/release-rust.yml` (X11, DBus, udev, GTK/tray, GStreamer, USB/input-related libraries, and others). The app exposes a `scrap-pkg-config` feature for using system packages rather than vcpkg on Linux. Match the repository's Nix or CI build setup when diagnosing native dependency failures.

The release workflow currently defines Linux and Windows build jobs, configures vcpkg for the relevant platform, and runs `cargo build --release --verbose`. CI trigger branch configuration is in the workflow itself; do not assume it follows the active feature branch or a requested PR base.

## Engineering guidance

1. **Read local source before changing behavior.** This guide is an index and high-level map, not a substitute for inspecting the target code and tests.
2. **Keep the four-zone contract.** Every RGB write or serialized custom frame must preserve the exact 12-byte layout.
3. **Respect the two effect execution models.** Hardware effects configure the driver's firmware mode; software effects configure Static and generate RGB frames.
4. **Preserve stop semantics.** Manager work is synchronous on one worker. A blocking effect that fails to poll cancellation prevents profile replacement and shutdown.
5. **Keep all surfaces in sync.** New effects affect enum serialization, `EnumIter` UI/CLI listing, helper capabilities, GUI controls, and exhaustive runtime dispatch.
6. **Treat input as optional.** Input setup is deliberately fallible; input-reactive behavior must have a graceful fallback.
7. **Keep dependencies pinned and native requirements explicit.** Windows requires MSVC, libclang, and the project's vcpkg libraries for `scrap`.
8. **Avoid unrelated workspace/user state.** `target/` is ignored build output; do not delete or overwrite user profiles/settings or unrelated untracked files.
9. **Use focused checks, then validate the exact deliverable.** Formatting and tests are useful, but a successful build or test run alone does not prove a copied `output-app/app.exe` exists or matches the final release binary.
10. **Keep this reference current.** When changing core crate boundaries, device contracts, effect architecture, build instructions, or hook behavior, update this document and relevant user-facing documentation in the same change.
