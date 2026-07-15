//! Executable Tauri 2 contract used only by native continuous integration.

#[tauri::command]
fn health() -> &'static str {
    "ok"
}

fn configure<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![health])
}

/// Constructs the native Tauri builder compiled by every desktop CI lane.
pub fn native_builder() -> tauri::Builder<tauri::Wry> {
    configure(tauri::Builder::default())
}

#[cfg(test)]
mod tests {
    use super::configure;

    #[test]
    fn command_builder_constructs_with_the_mock_runtime() {
        let app = configure(tauri::test::mock_builder())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock Tauri application should build");

        assert_eq!(app.package_info().name, "test");
    }
}
