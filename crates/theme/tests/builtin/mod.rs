use super::*;
use crate::rgb::Rgb;

fn hex(s: &str) -> Rgb {
    s.parse().unwrap()
}

#[test]
fn lookup_dark_and_light() {
    assert_eq!(builtin("dark").unwrap().name, "dark");
    assert_eq!(builtin("light").unwrap().name, "light");
}

#[test]
fn lookup_is_case_insensitive() {
    assert_eq!(builtin("DARK").unwrap().name, "dark");
    assert_eq!(builtin("DRACULA").unwrap().name, "Dracula");
    assert_eq!(builtin("nord").unwrap().name, "Nord");
}

#[test]
fn lookup_ignores_separators() {
    for query in [
        "Catppuccin Mocha",
        "catppuccin-mocha",
        "catppuccin_mocha",
        "catppuccinmocha",
        "CATPPUCCIN-MOCHA",
    ] {
        assert_eq!(builtin(query).unwrap().name, "Catppuccin Mocha", "{query}");
    }
    assert_eq!(builtin("gruvbox dark").unwrap().name, "Gruvbox Dark");
    assert_eq!(builtin("tokyo-night").unwrap().name, "Tokyo Night");
    assert_eq!(builtin("one_dark").unwrap().name, "One Dark");
    assert_eq!(builtin("github-light").unwrap().name, "GitHub Light");
    assert_eq!(builtin("rose pine").unwrap().name, "Rose Pine");
}

#[test]
fn lookup_unknown_is_none() {
    assert!(builtin("solarized").is_none());
    assert!(builtin("gruvbox").is_none());
    assert!(builtin("").is_none());
}

#[test]
fn names_sorted_and_complete() {
    let n = names();
    assert_eq!(n.len(), ALL.len());
    assert_eq!(n.len(), 22);
    let mut sorted = n.clone();
    sorted.sort_unstable();
    assert_eq!(n, sorted);
    assert!(n.contains(&"Dracula"));
    assert!(n.contains(&"dark"));
}

#[test]
fn normalized_names_are_unique() {
    for (i, a) in ALL.iter().enumerate() {
        for b in &ALL[i + 1..] {
            assert_ne!(
                normalize(a.name),
                normalize(b.name),
                "{} vs {}",
                a.name,
                b.name
            );
        }
    }
}

#[test]
fn default_is_dark() {
    assert_eq!(default_scheme().name, "dark");
    assert!(default_scheme().is_dark());
}

#[test]
fn background_spot_checks() {
    let cases = [
        ("dracula", "#282a36"),
        ("nord", "#2e3440"),
        ("gruvboxdark", "#282828"),
        ("gruvboxlight", "#fbf1c7"),
        ("solarizeddark", "#002b36"),
        ("solarizedlight", "#fdf6e3"),
        ("catppuccinmocha", "#1e1e2e"),
        ("catppuccinlatte", "#eff1f5"),
        ("tokyonight", "#1a1b26"),
        ("onedark", "#282c34"),
        ("monokai", "#272822"),
        ("ayudark", "#0a0e14"),
        ("rosepine", "#191724"),
        ("kanagawa", "#1f1f28"),
        ("everforest", "#2d353b"),
        ("githubdark", "#0d1117"),
        ("githublight", "#ffffff"),
        ("materialdark", "#263238"),
        ("palenight", "#292d3e"),
        ("zenburn", "#3f3f3f"),
    ];
    for (name, bg) in cases {
        assert_eq!(builtin(name).unwrap().background, hex(bg), "{name}");
    }
}

#[test]
fn foreground_spot_checks() {
    let cases = [
        ("dracula", "#f8f8f2"),
        ("nord", "#d8dee9"),
        ("gruvboxdark", "#ebdbb2"),
        ("solarizeddark", "#839496"),
        ("catppuccinmocha", "#cdd6f4"),
        ("tokyonight", "#c0caf5"),
        ("onedark", "#abb2bf"),
        ("rosepine", "#e0def4"),
        ("kanagawa", "#dcd7ba"),
        ("everforest", "#d3c6aa"),
        ("zenburn", "#dcdccc"),
    ];
    for (name, fg) in cases {
        assert_eq!(builtin(name).unwrap().foreground, hex(fg), "{name}");
    }
}

#[test]
fn ansi_red_spot_checks() {
    let cases = [
        ("dracula", "#ff5555"),
        ("nord", "#bf616a"),
        ("gruvboxdark", "#cc241d"),
        ("solarizeddark", "#dc322f"),
        ("catppuccinmocha", "#f38ba8"),
        ("catppuccinlatte", "#d20f39"),
        ("tokyonight", "#f7768e"),
        ("onedark", "#e06c75"),
        ("monokai", "#f92672"),
        ("ayudark", "#ea6c73"),
        ("rosepine", "#eb6f92"),
        ("kanagawa", "#c34043"),
        ("everforest", "#e67e80"),
        ("githubdark", "#ff7b72"),
        ("githublight", "#cf222e"),
        ("materialdark", "#ff5370"),
        ("palenight", "#f07178"),
        ("zenburn", "#705050"),
    ];
    for (name, red) in cases {
        assert_eq!(builtin(name).unwrap().ansi[1], hex(red), "{name}");
    }
}

#[test]
fn darkness_matches_variant() {
    let lights = [
        "light",
        "gruvboxlight",
        "solarizedlight",
        "catppuccinlatte",
        "githublight",
    ];
    for scheme in ALL {
        let expect_light = lights
            .iter()
            .any(|l| builtin(l).unwrap().name == scheme.name);
        assert_eq!(scheme.is_dark(), !expect_light, "{}", scheme.name);
    }
}
