fn main() -> eframe::Result<()> {
    #[cfg(target_os = "android")]
    {
        kumir_gui::run_native(None)
    }
    #[cfg(not(target_os = "android"))]
    {
        kumir_gui::run_native()
    }
}
