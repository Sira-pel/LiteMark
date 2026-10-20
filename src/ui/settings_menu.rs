use gpui::prelude::*;
use gpui::*;
use crate::theme::{sz, Theme, ThemeMode};
use super::menu::MenuItem;
use super::primitives::render_icon;

#[allow(dead_code)]
pub struct SettingsMenuProps<FToggleTheme, FSetFontSize, FHelp, FAbout, FTogOpen, FClose>
where
    FToggleTheme: Fn(ThemeMode, &mut App) + 'static + Clone,
    FSetFontSize: Fn(f32, &mut App) + 'static + Clone,
    FHelp: Fn(&mut App) + 'static + Clone,
    FAbout: Fn(&mut App) + 'static + Clone,
    FTogOpen: Fn(&mut App) + 'static + Clone,
    FClose: Fn(&mut App) + 'static + Clone,
{
    pub is_open: bool,
    pub current_theme: ThemeMode,
    pub base_font_size: f32,
    pub on_set_theme: FToggleTheme,
    pub on_set_font_size: FSetFontSize,
    pub on_open_help: FHelp,
    pub on_open_about: FAbout,
    pub on_toggle_open: FTogOpen,
    pub on_close: FClose,
}

pub fn render_settings_button<FTogOpen>(
    is_open: bool,
    on_toggle_open: FTogOpen,
    theme: &Theme,
) -> impl IntoElement
where
    FTogOpen: Fn(&mut App) + 'static + Clone,
{
    let tog = on_toggle_open.clone();

    div()
        .id("toolbar-settings-gear")
        .flex()
        .items_center()
        .justify_center()
        .w(sz::BTN)
        .h(sz::TB_TILE_H)
        .rounded(sz::R_MD)
        .cursor_pointer()
        .bg(if is_open { theme.border_subtle } else { rgba(0x00000000) })
        .text_color(if is_open { theme.text_strong } else { theme.text_muted })
        .hover(|h| h.bg(theme.tile_hover).text_color(theme.text_hi))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| tog(cx))
        .child(render_icon("settings", 16.0, if is_open { theme.text_strong } else { theme.text_muted }))
}

pub fn build_settings_menu_items(
    current_theme: ThemeMode,
    base_font_size: f32,
    on_set_theme: impl Fn(ThemeMode, &mut App) + 'static + Clone,
    on_set_font_size: impl Fn(f32, &mut App) + 'static + Clone,
    on_open_help: impl Fn(&mut App) + 'static + Clone,
    on_open_about: impl Fn(&mut App) + 'static + Clone,
) -> Vec<MenuItem> {
    let t_dark = on_set_theme.clone();
    let t_light = on_set_theme.clone();
    let t_sys = on_set_theme.clone();

    let font_sizes: &[(f32, &str)] = &[
        (13.0, "13 px"),
        (14.0, "14 px"),
        (15.0, "15 px"),
        (16.0, "16 px"),
        (17.0, "17 px (Default)"),
        (18.0, "18 px"),
        (19.0, "19 px"),
        (20.0, "20 px"),
        (22.0, "22 px"),
        (24.0, "24 px"),
    ];

    let font_sub_items: Vec<MenuItem> = font_sizes
        .iter()
        .map(|&(size, label)| {
            let is_selected = (base_font_size - size).abs() < 0.5;
            let fs = on_set_font_size.clone();
            MenuItem::radio(label, is_selected, None, move |_, cx| fs(size, cx))
        })
        .collect();

    let current_size_label = format!("{} px", base_font_size as usize);

    vec![
        MenuItem::heading("THEME"),
        MenuItem::radio("Dark", current_theme == ThemeMode::YaruDark, None, move |_, cx| t_dark(ThemeMode::YaruDark, cx)),
        MenuItem::radio("Light", current_theme == ThemeMode::YaruLight, None, move |_, cx| t_light(ThemeMode::YaruLight, cx)),
        MenuItem::radio("System", current_theme == ThemeMode::System, None, move |_, cx| t_sys(ThemeMode::System, cx)),
        MenuItem::divider(),
        MenuItem {
            label: "Font size",
            icon: None,
            shortcut: None,
            kind: super::menu::MenuItemKind::Submenu(font_sub_items),
            disabled: false,
            hint: Some(Box::leak(current_size_label.into_boxed_str())),
            action: std::sync::Arc::new(|_, _| {}),
        },
        MenuItem::divider(),
        MenuItem::action("Help & shortcuts", None, Some("F1"), {
            let h = on_open_help.clone();
            move |_, cx| h(cx)
        }),
        MenuItem::action("About LiteMark", None, None, {
            let a = on_open_about.clone();
            move |_, cx| a(cx)
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::build_settings_menu_items;
    use crate::theme::ThemeMode;
    use crate::ui::MenuItemKind;

    #[test]
    fn test_font_size_options_and_selection() {
        let items = build_settings_menu_items(
            ThemeMode::YaruDark,
            17.0,
            |_, _| {},
            |_, _| {},
            |_| {},
            |_| {},
        );

        let font_item = items.iter().find(|i| i.label == "Font size").expect("Font size menu item exists");
        assert_eq!(font_item.hint, Some("17 px"));

        if let MenuItemKind::Submenu(sub) = &font_item.kind {
            let labels: Vec<&str> = sub.iter().map(|s| s.label).collect();
            assert_eq!(
                labels,
                vec![
                    "13 px", "14 px", "15 px", "16 px", "17 px (Default)",
                    "18 px", "19 px", "20 px", "22 px", "24 px"
                ]
            );

            // 17 px should be checked
            let checked_17 = sub.iter().find(|s| s.label == "17 px (Default)").unwrap();
            assert!(matches!(checked_17.kind, MenuItemKind::Radio(true)));

            // 15 px should not be checked
            let checked_15 = sub.iter().find(|s| s.label == "15 px").unwrap();
            assert!(matches!(checked_15.kind, MenuItemKind::Radio(false)));
        } else {
            panic!("Expected submenu for Font size");
        }
    }
}
