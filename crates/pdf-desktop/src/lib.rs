mod commands;
mod menu;
mod state;

use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(state::AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let menu = menu::build_menu(&handle)?;
            app.set_menu(menu)?;

            // Forward menu events to the frontend.
            app.on_menu_event(move |app_handle, event| {
                let id = event.id().0.as_str();
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.emit("menu-event", id);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_document,
            commands::close_document,
            commands::page_count,
            commands::render_page,
            commands::render_thumbnail,
            commands::document_info,
            commands::get_page_geometry,
            commands::get_bookmarks,
            commands::rotate_page,
            commands::delete_page,
            commands::extract_page_text,
            commands::extract_text_blocks,
            commands::search_document,
            commands::print_document,
            commands::save_document,
            commands::save_document_as,
            commands::undo_document,
            commands::redo_document,
            commands::is_document_dirty,
            commands::add_annotation,
            commands::delete_annotation,
            commands::list_annotations,
        ])
        .run(tauri::generate_context!())
        .expect("error running XFA PDF Viewer");
}
