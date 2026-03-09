use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Wry,
};

/// Build the application menu bar.
pub fn build_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<Wry>> {
    let file_menu = SubmenuBuilder::new(app, "File")
        .items(&[
            &MenuItemBuilder::new("Open...")
                .id("open")
                .accelerator("CmdOrCtrl+O")
                .build(app)?,
            &MenuItemBuilder::new("Close Tab")
                .id("close_tab")
                .accelerator("CmdOrCtrl+W")
                .build(app)?,
            &MenuItemBuilder::new("Save")
                .id("save")
                .accelerator("CmdOrCtrl+S")
                .build(app)?,
            &MenuItemBuilder::new("Save As...")
                .id("save_as")
                .accelerator("CmdOrCtrl+Shift+S")
                .build(app)?,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &MenuItemBuilder::new("Print...")
                .id("print")
                .accelerator("CmdOrCtrl+P")
                .build(app)?,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &tauri::menu::PredefinedMenuItem::quit(app, Some("Quit"))?,
        ])
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .items(&[
            &MenuItemBuilder::new("Undo")
                .id("undo")
                .accelerator("CmdOrCtrl+Z")
                .build(app)?,
            &MenuItemBuilder::new("Redo")
                .id("redo")
                .accelerator("CmdOrCtrl+Shift+Z")
                .build(app)?,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &MenuItemBuilder::new("Copy")
                .id("copy")
                .accelerator("CmdOrCtrl+C")
                .build(app)?,
            &MenuItemBuilder::new("Select All")
                .id("select_all")
                .accelerator("CmdOrCtrl+A")
                .build(app)?,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &MenuItemBuilder::new("Find...")
                .id("find")
                .accelerator("CmdOrCtrl+F")
                .build(app)?,
        ])
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View")
        .items(&[
            &MenuItemBuilder::new("Zoom In")
                .id("zoom_in")
                .accelerator("CmdOrCtrl+=")
                .build(app)?,
            &MenuItemBuilder::new("Zoom Out")
                .id("zoom_out")
                .accelerator("CmdOrCtrl+-")
                .build(app)?,
            &MenuItemBuilder::new("Fit to Width")
                .id("fit_width")
                .accelerator("CmdOrCtrl+1")
                .build(app)?,
            &MenuItemBuilder::new("Fit to Page")
                .id("fit_page")
                .accelerator("CmdOrCtrl+2")
                .build(app)?,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &MenuItemBuilder::new("Toggle Sidebar")
                .id("toggle_sidebar")
                .accelerator("CmdOrCtrl+B")
                .build(app)?,
            &MenuItemBuilder::new("Toggle Dark Mode")
                .id("toggle_dark")
                .accelerator("CmdOrCtrl+D")
                .build(app)?,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &tauri::menu::PredefinedMenuItem::fullscreen(app, Some("Full Screen"))?,
        ])
        .build()?;

    let tools_menu = SubmenuBuilder::new(app, "Tools")
        .items(&[&MenuItemBuilder::new("Document Info...")
            .id("doc_info")
            .accelerator("CmdOrCtrl+I")
            .build(app)?])
        .build()?;

    let help_menu = SubmenuBuilder::new(app, "Help")
        .items(&[&MenuItemBuilder::new("About XFA PDF Viewer")
            .id("about")
            .build(app)?])
        .build()?;

    MenuBuilder::new(app)
        .items(&[&file_menu, &edit_menu, &view_menu, &tools_menu, &help_menu])
        .build()
}
