//! Manual check that the dashboard's own display enumeration matches the real
//! desktop. Ignored by default because it needs an interactive Windows session
//! (and specific hardware); run it with:
//!
//! ```text
//! cargo test --release --test displays_live prints_the_active_displays -- --ignored --nocapture
//! ```

use perfwindow::displays;

#[test]
#[ignore = "reads the live display topology; run manually in an interactive session"]
fn prints_the_active_displays() {
    let displays = displays::enumerate();
    for d in &displays {
        println!(
            "{} | {} | {}x{} @ {}Hz | {},{} | {}",
            d.gdi_name,
            d.model.as_deref().unwrap_or("(no model)"),
            d.width,
            d.height,
            d.refresh.label(),
            d.position.0,
            d.position.1,
            if d.primary { "primary" } else { "secondary" },
        );
    }

    // Ground truth for the development machine: an NVIDIA-driven 1080p panel
    // at 75 Hz on the primary source and an Intel-driven 1080p laptop panel at
    // 60 Hz to the left of it. Session-0 enumeration would report one
    // virtualised 1024x768@60 display instead.
    assert_eq!(
        displays.len(),
        2,
        "expected exactly the two monitors under test, got {}",
        displays.len()
    );

    let primary = displays
        .iter()
        .find(|d| d.primary)
        .expect("one display should be primary");
    assert_eq!((primary.width, primary.height), (1920, 1080));
    assert_eq!(primary.refresh.label(), "75");
    assert_eq!(primary.position, (0, 0));
    assert_eq!(primary.gdi_name, "\\\\.\\DISPLAY5");

    let secondary = displays
        .iter()
        .find(|d| !d.primary)
        .expect("one display should be secondary");
    assert_eq!((secondary.width, secondary.height), (1920, 1080));
    assert_eq!(secondary.refresh.label(), "60");
    assert_eq!(secondary.position, (-1920, 0));
    assert_eq!(secondary.gdi_name, "\\\\.\\DISPLAY1");
}
