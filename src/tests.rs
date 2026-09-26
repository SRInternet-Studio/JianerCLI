//! JianerCLI 核心逻辑单元测试。

#[cfg(test)]
mod unit_tests {
    use crate::local::{is_valid_plugin_id, parse_plugin_meta_source};
    use crate::market::client::{hex_sha256, sanitize_filename};
    use crate::market::index::SemVer;

    #[test]
    fn parses_single_file_metadata_without_importing_python() {
        let source = r#"
from jianer.plugins import PluginMetadata
__plugin_meta__ = PluginMetadata(
    name="jianerbot-plugin-example",
    description="An example, with commas, too",
    usage=("line one\\n" "line two"),
    requires={"jianerbot-plugin-alconna", "jianerbot-plugin-other"},
)
"#;
        let meta = parse_plugin_meta_source(source).unwrap();
        assert_eq!(meta.name, "jianerbot-plugin-example");
        assert!(meta.description.contains("commas"));
        assert_eq!(meta.requires.len(), 2);
        assert!(meta
            .requires
            .contains(&"jianerbot-plugin-alconna".to_string()));
    }

    #[test]
    fn ignores_indented_fake_metadata() {
        let source = "def f():\n    __plugin_meta__ = PluginMetadata(name='fake')\n";
        assert!(parse_plugin_meta_source(source).is_none());
    }

    #[test]
    fn plugin_ids_follow_core_rule() {
        assert!(is_valid_plugin_id("jianerbot-plugin-abc"));
        assert!(is_valid_plugin_id("jianerbot-plugin-a1-b2"));
        assert!(!is_valid_plugin_id("abc"));
        assert!(!is_valid_plugin_id("jianerbot-plugin-A"));
        assert!(!is_valid_plugin_id("jianerbot-plugin-a--b"));
    }

    #[test]
    fn semver_release_is_newer_than_prerelease() {
        assert!(SemVer::parse("1.0.0").is_newer_than(&SemVer::parse("1.0.0-beta")));
        assert!(!SemVer::parse("1.0.0-beta").is_newer_than(&SemVer::parse("1.0.0")));
        assert!(SemVer::parse("1.2.0").is_newer_than(&SemVer::parse("1.1.9")));
    }

    #[test]
    fn zip_names_cannot_escape_cache() {
        assert_eq!(sanitize_filename("../../evil.zip"), "evil.zip");
        assert_eq!(sanitize_filename("C:\\\\evil.zip"), "evil.zip");
    }

    #[test]
    fn sha256_is_stable() {
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn market_repository_builds_asset_url() {
        let repository = crate::market::Repository {
            url: "https://github.com/example/market".to_string(),
            r#ref: "abc".to_string(),
            release_url_template: Some("https://example/{tag}/{asset}".to_string()),
        };
        assert_eq!(
            repository.asset_url("x@1.0.0", "x.zip"),
            "https://example/x@1.0.0/x.zip"
        );
        assert_eq!(repository.github_slug(), Some("example/market".to_string()));
    }

    #[test]
    fn parser_handles_real_bot_style_metadata() {
        let source = include_str!("../../Jianer_Canary/Jianer_QQ_bot/plugins/LikePlugin.py");
        let meta = parse_plugin_meta_source(source).unwrap();
        assert_eq!(meta.name, "jianerbot-plugin-like");
        assert_eq!(meta.requires, vec!["jianerbot-plugin-alconna"]);
    }
}
