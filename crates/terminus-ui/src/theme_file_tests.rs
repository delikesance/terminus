use super::*;

fn none(_: &str) -> Option<ThemeFile> {
    None
}

fn resolve(src: &str) -> Result<Resolved, ThemeError> {
    ChromeTheme::from_file(&ThemeFile::parse(src).unwrap(), &none)
}

#[test]
fn empty_file_is_violet_ink() {
    let r = resolve("").unwrap();
    assert_eq!(r.theme, ChromeTheme::violet_ink());
    assert!(r.warnings.is_empty());
}

#[test]
fn extending_paper_without_overrides_reproduces_it() {
    let r = resolve("[meta]\nextends = \"violet-paper\"").unwrap();
    assert_eq!(r.theme, ChromeTheme::violet_paper());
}

#[test]
fn overrides_and_variables_apply_and_rederive_legacy() {
    let r = resolve(
        "[palette]\nbg = \"#282828\"\nalias = \"$bg\"\n[chrome]\nframe = \"$alias\"\ntext = \"#ebdbb2\"",
    )
    .unwrap();
    assert_eq!(r.theme.frame, rgba([0x28, 0x28, 0x28], 1.0));
    assert_eq!(r.theme.panel_bg, r.theme.frame);
    assert_eq!(r.theme.shell_bg, r.theme.frame);
    assert_eq!(r.theme.text, [0xeb, 0xdb, 0xb2, 255]);
}

#[test]
fn alpha_channel_is_parsed() {
    let r = resolve("[chrome]\nscrim = \"#00000080\"").unwrap();
    assert!((r.theme.scrim[3] - 128.0 / 255.0).abs() < 1e-6);
}

#[test]
fn errors_are_reported() {
    assert!(matches!(
        resolve("[chrome]\nframe = \"red\""),
        Err(ThemeError::BadColor { .. })
    ));
    assert_eq!(
        resolve("[chrome]\nframe = \"$nope\""),
        Err(ThemeError::UnknownVariable("nope".into()))
    );
    assert_eq!(
        resolve("[chrome]\nbogus = \"#000000\""),
        Err(ThemeError::UnknownSlot("bogus".into()))
    );
    assert_eq!(
        resolve("[meta]\nextends = \"ghost\""),
        Err(ThemeError::UnknownBase("ghost".into()))
    );
    assert!(matches!(
        ThemeFile::parse("[chrome"),
        Err(ThemeError::Syntax(_))
    ));
}

#[test]
fn extends_chain_child_wins_and_cycles_stop() {
    let parent = ThemeFile::parse(
        "[palette]\nc = \"#111111\"\n[chrome]\nframe = \"$c\"\naccent = \"#ff0000\"",
    )
    .unwrap();
    let lookup = |n: &str| (n == "parent").then(|| parent.clone());
    let child =
        ThemeFile::parse("[meta]\nextends = \"parent\"\n[chrome]\naccent = \"#00ff00\"")
            .unwrap();
    let t = ChromeTheme::from_file(&child, &lookup).unwrap().theme;
    assert_eq!(t.frame, rgba([0x11, 0x11, 0x11], 1.0));
    assert_eq!(t.accent, rgba([0, 0xff, 0], 1.0));

    let looping = ThemeFile::parse("[meta]\nextends = \"loop\"").unwrap();
    let lookup = |_: &str| Some(looping.clone());
    assert_eq!(
        ChromeTheme::from_file(&looping, &lookup),
        Err(ThemeError::ExtendsTooDeep)
    );
}

#[test]
fn low_contrast_is_a_warning_not_an_error() {
    let r = resolve("[chrome]\ntext = \"#14111c\"").unwrap();
    assert!(r.warnings.iter().any(|w| w.starts_with("text on frame")));
}

#[test]
fn terminal_colors_table_is_ignored() {
    let r = resolve("[colors]\nbackground = \"#000000\"").unwrap();
    assert_eq!(r.theme, ChromeTheme::violet_ink());
}
