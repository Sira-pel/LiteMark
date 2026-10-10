export const C_GTK4_SOURCE = `/*
 * Ubuntu Lightweight Text Editor in C & GTK4
 * Minimalist GUI with Bold, Italic, Underline, Markdown formatting, Open/Save.
 *
 * Requirements on Ubuntu:
 *   sudo apt update
 *   sudo apt install build-essential libgtk-4-dev
 *
 * Compile:
 *   gcc $(pkg-config --cflags gtk4) main.c -o ubuntu-notepad $(pkg-config --libs gtk4)
 * Run:
 *   ./ubuntu-notepad
 */

#include <gtk/gtk.h>

static GtkWidget *text_view;
static GtkTextBuffer *text_buffer;
static GFile *current_file = NULL;
static GtkWidget *window;

static void apply_tag_to_selection(const char *tag_name) {
    GtkTextIter start, end;
    if (gtk_text_buffer_get_selection_bounds(text_buffer, &start, &end)) {
        gtk_text_buffer_apply_tag_by_name(text_buffer, tag_name, &start, &end);
    }
}

static void on_bold_clicked(GtkWidget *btn, gpointer data) {
    apply_tag_to_selection("bold");
}

static void on_italic_clicked(GtkWidget *btn, gpointer data) {
    apply_tag_to_selection("italic");
}

static void on_underline_clicked(GtkWidget *btn, gpointer data) {
    apply_tag_to_selection("underline");
}

static void on_header_clicked(GtkWidget *btn, gpointer data) {
    apply_tag_to_selection("header");
}

static void on_open_file_response(GObject *source, GAsyncResult *res, gpointer data) {
    GtkFileDialog *dialog = GTK_FILE_DIALOG(source);
    GError *error = NULL;
    GFile *file = gtk_file_dialog_open_finish(dialog, res, &error);

    if (file) {
        char *contents = NULL;
        gsize length = 0;
        if (g_file_load_contents(file, NULL, &contents, &length, NULL, &error)) {
            gtk_text_buffer_set_text(text_buffer, contents, length);
            g_free(contents);
            current_file = file;
            char *basename = g_file_get_basename(file);
            gtk_window_set_title(GTK_WINDOW(window), basename);
            g_free(basename);
        }
    }
}

static void on_open_clicked(GtkWidget *btn, gpointer data) {
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog, "Open Text or Markdown File");
    gtk_file_dialog_open(dialog, GTK_WINDOW(window), NULL, on_open_file_response, NULL);
}

static void on_save_file_response(GObject *source, GAsyncResult *res, gpointer data) {
    GtkFileDialog *dialog = GTK_FILE_DIALOG(source);
    GError *error = NULL;
    GFile *file = gtk_file_dialog_save_finish(dialog, res, &error);

    if (file) {
        GtkTextIter start, end;
        gtk_text_buffer_get_bounds(text_buffer, &start, &end);
        char *text = gtk_text_buffer_get_text(text_buffer, &start, &end, FALSE);

        if (g_file_replace_contents(file, text, strlen(text), NULL, FALSE, G_FILE_CREATE_NONE, NULL, NULL, &error)) {
            current_file = file;
            char *basename = g_file_get_basename(file);
            gtk_window_set_title(GTK_WINDOW(window), basename);
            g_free(basename);
        }
        g_free(text);
    }
}

static void on_save_clicked(GtkWidget *btn, gpointer data) {
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog, "Save Text or Markdown Document");
    gtk_file_dialog_save(dialog, GTK_WINDOW(window), NULL, on_save_file_response, NULL);
}

static void activate(GtkApplication *app, gpointer user_data) {
    window = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window), "Ubuntu Notepad");
    gtk_window_set_default_size(GTK_WINDOW(window), 800, 600);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_window_set_child(GTK_WINDOW(window), box);

    // Header Bar
    GtkWidget *header = gtk_header_bar_new();
    gtk_window_set_titlebar(GTK_WINDOW(window), header);

    // Open & Save
    GtkWidget *open_btn = gtk_button_new_with_label("Open");
    g_signal_connect(open_btn, "clicked", G_CALLBACK(on_open_clicked), NULL);
    gtk_header_bar_pack_start(GTK_HEADER_BAR(header), open_btn);

    GtkWidget *save_btn = gtk_button_new_with_label("Save");
    g_signal_connect(save_btn, "clicked", G_CALLBACK(on_save_clicked), NULL);
    gtk_header_bar_pack_start(GTK_HEADER_BAR(header), save_btn);

    // Formatting Toolbar
    GtkWidget *toolbar = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_widget_set_margin_start(toolbar, 10);
    gtk_widget_set_margin_end(toolbar, 10);
    gtk_widget_set_margin_top(toolbar, 6);
    gtk_widget_set_margin_bottom(toolbar, 6);

    GtkWidget *bold_btn = gtk_button_new_with_label("B");
    g_signal_connect(bold_btn, "clicked", G_CALLBACK(on_bold_clicked), NULL);
    gtk_box_append(GTK_BOX(toolbar), bold_btn);

    GtkWidget *italic_btn = gtk_button_new_with_label("I");
    g_signal_connect(italic_btn, "clicked", G_CALLBACK(on_italic_clicked), NULL);
    gtk_box_append(GTK_BOX(toolbar), italic_btn);

    GtkWidget *underline_btn = gtk_button_new_with_label("U");
    g_signal_connect(underline_btn, "clicked", G_CALLBACK(on_underline_clicked), NULL);
    gtk_box_append(GTK_BOX(toolbar), underline_btn);

    GtkWidget *header_btn = gtk_button_new_with_label("Header");
    g_signal_connect(header_btn, "clicked", G_CALLBACK(on_header_clicked), NULL);
    gtk_box_append(GTK_BOX(toolbar), header_btn);

    gtk_box_append(GTK_BOX(box), toolbar);

    // Text View & Scrolled Window
    GtkWidget *scrolled = gtk_scrolled_window_new();
    gtk_widget_set_vexpand(scrolled, TRUE);
    gtk_widget_set_hexpand(scrolled, TRUE);

    text_view = gtk_text_view_new();
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(text_view), GTK_WRAP_WORD_CHAR);
    gtk_text_view_set_left_margin(GTK_TEXT_VIEW(text_view), 16);
    gtk_text_view_set_right_margin(GTK_TEXT_VIEW(text_view), 16);
    gtk_text_view_set_top_margin(GTK_TEXT_VIEW(text_view), 12);
    gtk_text_view_set_bottom_margin(GTK_TEXT_VIEW(text_view), 12);

    text_buffer = gtk_text_view_get_buffer(GTK_TEXT_VIEW(text_view));

    // Register Text Tags
    gtk_text_buffer_create_tag(text_buffer, "bold", "weight", PANGO_WEIGHT_BOLD, NULL);
    gtk_text_buffer_create_tag(text_buffer, "italic", "style", PANGO_STYLE_ITALIC, NULL);
    gtk_text_buffer_create_tag(text_buffer, "underline", "underline", PANGO_UNDERLINE_SINGLE, NULL);
    gtk_text_buffer_create_tag(text_buffer, "header", "scale", 1.5, "weight", PANGO_WEIGHT_BOLD, NULL);

    gtk_text_buffer_set_text(text_buffer, "# Welcome to Ubuntu Notepad\\n\\nStart typing your markdown or plain text...", -1);

    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scrolled), text_view);
    gtk_box_append(GTK_BOX(box), scrolled);

    gtk_window_present(GTK_WINDOW(window));
}

int main(int argc, char **argv) {
    GtkApplication *app;
    int status;

    app = gtk_application_new("org.ubuntu.lightweight.notepad", G_APPLICATION_DEFAULT_FLAGS);
    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);

    return status;
}
`;

export const C_MAKEFILE = `CC = gcc
CFLAGS = $(shell pkg-config --cflags gtk4) -O2 -Wall
LIBS = $(shell pkg-config --libs gtk4)
TARGET = ubuntu-notepad

all: $(TARGET)

$(TARGET): main.c
	$(CC) $(CFLAGS) main.c -o $(TARGET) $(LIBS)

clean:
	rm -f $(TARGET)

install: $(TARGET)
	install -d $(DESTDIR)/usr/local/bin
	install -m 755 $(TARGET) $(DESTDIR)/usr/local/bin/
`;

export const RUST_CARGO_TOML = `[package]
name = "ubuntu-notepad"
version = "0.1.0"
edition = "2021"
description = "Ultra-lightweight native Ubuntu text and markdown editor"
authors = ["Ubuntu User <user@ubuntu.com>"]
license = "MIT"

[dependencies]
gtk4 = { version = "0.9", features = ["v4_12"] }
glib = "0.20"

# Configuration for generating a native Ubuntu .deb package (via cargo-deb):
[package.metadata.deb]
maintainer = "Ubuntu Community <packages@ubuntu.com>"
copyright = "2026, Ubuntu Community"
license-file = ["LICENSE", "4"]
extended-description = """
Ultra-lightweight, memory-efficient native GNOME text editor
written in Rust and GTK4 with markdown formatting and fast startup.
"""
section = "editors"
priority = "optional"
assets = [
    ["target/release/ubuntu-notepad", "usr/bin/", "755"],
    ["ubuntu-notepad.desktop", "usr/share/applications/", "644"],
]
`;

export const UBUNTU_DESKTOP_ENTRY = `[Desktop Entry]
Name=Ubuntu Notepad
Comment=Lightweight native GTK4 Markdown & Text Editor
Exec=ubuntu-notepad %F
Terminal=false
Type=Application
Icon=text-editor
Categories=Utility;TextEditor;GTK;GNOME;
MimeType=text/plain;text/markdown;
Keywords=Text;Editor;Markdown;Notes;
StartupNotify=true
`;

export const RUST_MAIN_RS = `//! Ubuntu Lightweight Text Editor in Rust + GTK4
//! Ultra-low memory (~20MB), blazing fast startup, cross-platform.
//!
//! Build & Run on Ubuntu:
//!   sudo apt install libgtk-4-dev build-essential
//!   cargo run --release

use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, FileDialog, HeaderBar, Orientation,
    ScrolledWindow, TextBuffer, TextView, WrapMode, pango,
};
use std::fs;

const APP_ID: &str = "org.ubuntu.rust.notepad";

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Ubuntu Notepad (Rust)")
        .default_width(850)
        .default_height(600)
        .build();

    let root_box = GtkBox::new(Orientation::Vertical, 0);
    window.set_child(Some(&root_box));

    // Header Bar
    let header = HeaderBar::new();
    window.set_titlebar(Some(&header));

    let open_btn = Button::with_label("Open");
    let save_btn = Button::with_label("Save");
    header.pack_start(&open_btn);
    header.pack_start(&save_btn);

    // Toolbar
    let toolbar = GtkBox::new(Orientation::Horizontal, 6);
    toolbar.set_margin_start(10);
    toolbar.set_margin_end(10);
    toolbar.set_margin_top(6);
    toolbar.set_margin_bottom(6);

    let bold_btn = Button::with_label("B");
    let italic_btn = Button::with_label("I");
    let underline_btn = Button::with_label("U");
    let header_btn = Button::with_label("Header");

    toolbar.append(&bold_btn);
    toolbar.append(&italic_btn);
    toolbar.append(&underline_btn);
    toolbar.append(&header_btn);
    root_box.append(&toolbar);

    // Text View & Scrolled Window
    let scrolled = ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);

    let text_view = TextView::new();
    text_view.set_wrap_mode(WrapMode::WordChar);
    text_view.set_left_margin(16);
    text_view.set_right_margin(16);
    text_view.set_top_margin(12);
    text_view.set_bottom_margin(12);

    let buffer = text_view.buffer();

    // Style Tags
    let tag_table = buffer.tag_table();
    let bold_tag = gtk4::TextTag::builder().name("bold").weight(pango::Weight::Bold.into()).build();
    let italic_tag = gtk4::TextTag::builder().name("italic").style(pango::Style::Italic).build();
    let underline_tag = gtk4::TextTag::builder().name("underline").underline(pango::Underline::Single).build();
    let header_tag = gtk4::TextTag::builder().name("header").scale(1.5).weight(pango::Weight::Bold.into()).build();

    tag_table.add(&bold_tag);
    tag_table.add(&italic_tag);
    tag_table.add(&underline_tag);
    tag_table.add(&header_tag);

    buffer.set_text("# Welcome to Ubuntu Notepad (Rust)\\n\\nFast, safe, and minimal memory usage.");

    // Action handlers for formatting
    let apply_tag = {
        let buffer = buffer.clone();
        move |tag_name: &str| {
            if let Some((start, end)) = buffer.selection_bounds() {
                buffer.apply_tag_by_name(tag_name, &start, &end);
            }
        }
    };

    let b_tag = apply_tag.clone();
    bold_btn.connect_clicked(move |_| b_tag("bold"));

    let i_tag = apply_tag.clone();
    italic_btn.connect_clicked(move |_| i_tag("italic"));

    let u_tag = apply_tag.clone();
    underline_btn.connect_clicked(move |_| u_tag("underline"));

    let h_tag = apply_tag.clone();
    header_btn.connect_clicked(move |_| h_tag("header"));

    // File Open
    {
        let window = window.clone();
        let buffer = buffer.clone();
        open_btn.connect_clicked(move |_| {
            let dialog = FileDialog::builder().title("Open Text or Markdown File").build();
            let window_clone = window.clone();
            let buffer_clone = buffer.clone();
            dialog.open(Some(&window), None::<&gio::Cancellable>, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        if let Ok(content) = fs::read_to_string(&path) {
                            buffer_clone.set_text(&content);
                            if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
                                window_clone.set_title(Some(file_name));
                            }
                        }
                    }
                }
            });
        });
    }

    // File Save
    {
        let window = window.clone();
        let buffer = buffer.clone();
        save_btn.connect_clicked(move |_| {
            let dialog = FileDialog::builder().title("Save File").build();
            let window_clone = window.clone();
            let buffer_clone = buffer.clone();
            dialog.save(Some(&window), None::<&gio::Cancellable>, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let text = buffer_clone.text(&buffer_clone.start_iter(), &buffer_clone.end_iter(), false);
                        if fs::write(&path, text.as_str()).is_ok() {
                            if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
                                window_clone.set_title(Some(file_name));
                            }
                        }
                    }
                }
            });
        });
    }

    scrolled.set_child(Some(&text_view));
    root_box.append(&scrolled);

    window.present();
}
`;
