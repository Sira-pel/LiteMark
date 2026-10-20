# Theme

Theme lets a user switch the editor between Dark, Light, and System from the settings menu.

## Sub-features

- `theme-open` opens the settings menu from the gear button.
- `theme-dark` chooses Dark.
- `theme-light` chooses Light.
- `theme-system` chooses System.

## How to get to it (user POV)

- Choose the toolbar settings gear (`id` `toolbar-settings-gear`).
- In the menu under the heading THEME, choose Dark, Light, or System.

## Driving it with control-litemark

Preconditions:

- Theme is stored with the window session. This kit does not start that window.

- **Refuse the window.** Run `.cursor/skills/verify-litemark/scripts/refuse-gui.sh`. Exit code is 2. Stdout contains `command=not-run target/debug/litemark`.
- **Exact reasons.** Stdout contains `reason=notes_dir_unisolated path=dirs::document_dir()/Notes symbol=get_or_create_notes_dir file=src/services/fs_service.rs` and `reason=vulkan_icd_missing path=/usr/share/vulkan/icd.d`.
- **Proof.** Stop. Do not claim a theme change from a render dump. The dump has no color tokens.

## Gotchas

- README.md lists Ctrl+Shift+D for theme. `src/main.rs` does not bind that chord. The labels in code are `Dark`, `Light`, and `System` (`src/ui/settings_menu.rs`).
- The gear is an icon button, not a text label. Its element id is `toolbar-settings-gear`.
- GPUI does not expose ARIA to xdotool. There is no CDP port.
