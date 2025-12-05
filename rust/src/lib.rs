use godot::prelude::*;

struct MyExtension;

#[gdextension]
unsafe impl ExtensionLibrary for MyExtension {
    fn on_level_init(level: InitLevel) {
        if level == InitLevel::Scene {
            godot_print_rich!(
                "[b][color=green]{} v{}-{}[/color][/b]\n\
                 [b]作者:[/b] {}\n\
                 [b]描述:[/b] {}\n\
                 [b]构建时间:[/b] {}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                env!("VERGEN_CARGO_TARGET_TRIPLE"),
                env!("CARGO_PKG_AUTHORS"),
                env!("CARGO_PKG_DESCRIPTION"),
                env!("VERGEN_BUILD_TIMESTAMP")
            );
        }
    }
}
