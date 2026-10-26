use serde_json::json;

use super::*;

fn config(text: &str) -> Config {
    Config::parse(text).unwrap()
}

fn path(text: &str) -> SettingPath {
    text.parse().unwrap()
}

fn change(pairs: &[(&str, Value)]) -> Vec<(String, Value)> {
    pairs.iter().map(|(p, v)| ((*p).to_owned(), v.clone())).collect()
}

#[test]
fn paths_are_keys_joined_with_dots() {
    assert_eq!(path("import.settings.matchThreshold").to_string(), "import.settings.matchThreshold");
    assert!("".parse::<SettingPath>().is_err());
    assert!("import..x".parse::<SettingPath>().is_err());
    assert!(path("quotes.services.x.key").within(&path("quotes.services")));
    assert!(!path("quotes").within(&path("quotes.services")));
}

#[test]
fn defaults_fill_in_and_stored_values_show_through() {
    let c = config(r#"{"import": {"settings": {"matchThreshold": 2}}, "quotes": {"services": {"mine": {"securityUrl": "https://q.example/{symbol}", "format": "json", "pointers": {"price": "/p"}}}}}"#);
    let all = c.effective();
    assert_eq!(all["import"]["settings"]["matchThreshold"], 2);
    assert_eq!(all["import"]["settings"]["autoClearThreshold"], 6);
    assert_eq!(all["import"]["settings"]["atmFeeThreshold"], "2.00");
    assert_eq!(all["locale"], "C");
    assert_eq!(all["quotes"]["network"], false);
    assert_eq!(all["accountingPeriod"]["start"], "year");
    assert_eq!(all["docLinks"]["pathHead"], Value::Null);
    assert_eq!(all["reports"]["limits"]["seconds"], 120);
    assert_eq!(all["jsonRpc"]["maxRequestBytes"], 64 << 20);
    assert_eq!(all["quotes"]["services"]["mine"]["minIntervalSeconds"], 0, "entry fields have defaults too");
    assert_eq!(all["quotes"]["timeoutSeconds"], 30);
    assert_eq!(all["defaultCurrency"], Value::Null);
    assert_eq!(all["import"]["csvLayouts"], json!({}));
    assert_eq!(all["quotes"]["services"]["frankfurter"]["format"], "frankfurter", "default services stay beside added ones");
    assert_eq!(all["quotes"]["sources"]["currency"], json!(["frankfurter", "alphavantage"]));
    assert_eq!(c.setting(&path("import.settings.useBayes")).unwrap(), true);
    assert_eq!(c.setting(&path("quotes.services.nosuch")).unwrap(), Value::Null);
    assert!(c.setting(&path("import.nosuch")).is_err());
    assert!(c.problems().is_empty(), "{:?}", c.problems());
}

#[test]
fn secrets_are_shown_only_as_set_or_unset() {
    let c = config(r#"{"quotes": {"services": {"alphavantage": {"key": "SECRET-1"}}}}"#);
    let all = c.view(None).unwrap();
    assert_eq!(all["quotes"]["services"]["alphavantage"]["key"], json!({ "set": true }));
    assert_eq!(all["quotes"]["services"]["twelvedata"]["key"], json!({ "set": false }));
    assert!(!all.to_string().contains("SECRET-1"));
    let one = c.view(Some(&path("quotes.services.alphavantage"))).unwrap();
    assert_eq!(one["key"], json!({ "set": true }));
    assert_eq!(c.view(Some(&path("quotes.services.alphavantage.key"))).unwrap(), json!({ "set": true }));
    assert_eq!(c.setting(&path("quotes.services.alphavantage.key")).unwrap(), "SECRET-1", "red-ale itself reads it");
    let bad = config(r#"{"quotes": {"services": {"alphavantage": {"key": 12345}}}}"#);
    assert!(!format!("{:?}", bad.problems()).contains("12345"), "a bad secret is not echoed");
}

#[test]
fn unknown_keys_are_listed_and_kept() {
    let mut c = config(r#"{"other": {"x": 1}, "import": {"settings": {"typo": true}}, "reports": {"saved": [{"guid": "6f1e0c9d2b7a4e3f8a5b1c2d3e4f5a6b", "name": "n", "template": "c4173ac99b2b448289bf4d11c731af13", "extra": 1}]}}"#);
    assert_eq!(c.unknown_keys(), ["other", "import.settings.typo"]);
    c.apply(&change(&[("import.settings.matchThreshold", json!(2))])).unwrap();
    let text = c.to_text();
    assert!(text.contains("\"typo\"") && text.contains("\"other\"") && text.contains("\"extra\""), "{text}");
}

#[test]
fn null_restores_the_default_and_leaves_no_empty_objects() {
    let mut c = config("{}");
    c.apply(&change(&[("import.settings.matchThreshold", json!(5)), ("locale", json!("environment"))])).unwrap();
    assert_eq!(c.effective()["import"]["settings"]["matchThreshold"], 5);
    c.apply(&change(&[("import.settings.matchThreshold", Value::Null)])).unwrap();
    assert_eq!(c.effective()["import"]["settings"]["matchThreshold"], 1);
    assert_eq!(c.to_text(), "{\n  \"locale\": \"environment\"\n}\n");
}

#[test]
fn every_change_is_checked_before_any_is_made() {
    let mut c = config(r#"{"import": {"settings": {"matchThreshold": 2}}}"#);
    let before = c.clone();
    let e = c
        .apply(&change(&[
            ("import.settings.matchThreshold", json!(3)),
            ("import.settings.autoAddThreshold", json!("high")),
            ("locale", json!("xx_YY")),
            ("import.settings.atmFeeThreshold", json!("-1")),
            ("nosuch", json!(1)),
        ]))
        .unwrap_err();
    let paths: Vec<&str> = e.problems.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(paths, ["nosuch"], "an unknown path is refused before values are checked");
    let e = c.apply(&change(&[("import.settings.matchThreshold", json!(3)), ("import.settings.autoAddThreshold", json!("high")), ("locale", json!("xx_YY"))])).unwrap_err();
    let paths: Vec<&str> = e.problems.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(paths, ["locale", "import.settings.autoAddThreshold"]);
    assert_eq!(c, before);
    assert!(c.apply(&[]).is_err());
    assert!(c.apply(&change(&[("import", json!({})), ("import.settings.useBayes", json!(false))])).is_err(), "overlapping paths");
    assert!(c.apply(&change(&[("jsonRpc.maxRequestBytes", json!(1))])).is_err(), "below the minimum");
}

#[test]
fn owned_settings_are_refused() {
    let mut c = config("{}");
    let e = c.apply(&change(&[("reports.saved", json!([]))])).unwrap_err();
    assert!(e.problems[0].message.contains("own methods"), "{e}");
    assert!(c.apply(&change(&[("reports", Value::Null)])).is_err(), "a group holding an owned setting");
    c.apply(&change(&[("reports.dateFormat", json!("iso"))])).unwrap();
}

#[test]
fn sources_must_name_services() {
    let mut c = config("{}");
    let e = c.apply(&change(&[("quotes.sources.tsp", json!(["nosuch"]))])).unwrap_err();
    assert_eq!(e.problems[0].path, "quotes.sources.tsp[0]");
    let service = json!({ "securityUrl": "https://tsp.example/{symbol}", "format": "json", "pointers": { "price": "/price" } });
    c.apply(&change(&[("quotes.services.tsp", service), ("quotes.sources.tsp", json!(["tsp"]))])).unwrap();
    assert!(c.apply(&change(&[("quotes.services.bad", json!({ "format": "frankfurter" }))])).is_err(), "a service needs a URL");
    let no_price = json!({ "format": "json", "securityUrl": "https://q.example/{symbol}" });
    assert!(c.apply(&change(&[("quotes.services.bad", no_price)])).is_err(), "a json service needs a price pointer");
    let placeholder = json!({ "format": "finnhub", "securityUrl": "https://q.example/{ticker}" });
    assert!(c.apply(&change(&[("quotes.services.bad", placeholder)])).is_err(), "unknown placeholders are refused");
}

#[test]
fn problems_elsewhere_do_not_block_a_change() {
    let mut c = config(r#"{"import": {"settings": {"matchThreshold": "x"}}}"#);
    assert_eq!(c.problems().len(), 1);
    c.apply(&change(&[("locale", json!("C"))])).unwrap();
    assert!(c.apply(&change(&[("import.settings", json!({ "useBayes": false, "matchThreshold": "x" }))])).is_err(), "nor are they kept by one that touches them");
    assert!(c.checked("import.settings.matchThreshold").is_err());
    assert_eq!(c.checked("locale").unwrap(), "C");
}

#[test]
fn a_key_is_bound_to_its_service_host() {
    let mut c = config(r#"{"quotes": {"services": {"alphavantage": {"key": "K"}, "twelvedata": {"key": "T"}}}}"#);
    let moved = c.apply(&change(&[("quotes.services.alphavantage.securityUrl", json!("https://evil.example/q?k={key}"))])).unwrap();
    assert_eq!(moved.cleared_keys, ["quotes.services.alphavantage.key"]);
    assert_eq!(moved.refused_over_mcp, Vec::<String>::new(), "clearing by a move is not a change of the key");
    assert_eq!(c.setting(&path("quotes.services.alphavantage.key")).unwrap(), Value::Null);
    let same_host = c.apply(&change(&[("quotes.services.twelvedata.securityUrl", json!("https://api.twelvedata.com/price?symbol={symbol}&apikey={key}"))]));
    assert!(same_host.unwrap().cleared_keys.is_empty(), "the same host keeps its key");
    let downgrade = c.apply(&change(&[("quotes.services.twelvedata.securityUrl", json!("http://api.twelvedata.com/price?symbol={symbol}"))]));
    assert_eq!(downgrade.unwrap().cleared_keys, ["quotes.services.twelvedata.key"], "http is another origin");
    let both = c.apply(&change(&[("quotes.services.alphavantage", json!({ "format": "alphavantage", "securityUrl": "https://proxy.local/av", "key": "NEW" }))])).unwrap();
    assert!(both.cleared_keys.is_empty(), "a key given with the move is kept");
    assert_eq!(both.refused_over_mcp, ["quotes.services.alphavantage.key"]);
    assert_eq!(c.setting(&path("quotes.services.alphavantage.key")).unwrap(), "NEW");
}

#[test]
fn changes_only_a_person_may_make_are_reported() {
    let mut c = config("{}");
    let on = c.apply(&change(&[("quotes.network", json!(true))])).unwrap();
    assert_eq!(on.refused_over_mcp, ["quotes.network"]);
    let same = c.apply(&change(&[("quotes.network", json!(true))])).unwrap();
    assert!(same.refused_over_mcp.is_empty(), "no change, nothing to refuse");
    let key = c.apply(&change(&[("quotes.services.finnhub.key", json!("F"))])).unwrap();
    assert_eq!(key.refused_over_mcp, ["quotes.services.finnhub.key"]);
    let cleared = c.apply(&change(&[("quotes.services.finnhub.key", Value::Null)])).unwrap();
    assert_eq!(cleared.refused_over_mcp, ["quotes.services.finnhub.key"]);
    let added = c.apply(&change(&[("quotes.services.mine", json!({ "securityUrl": "https://m.example/{symbol}", "format": "json", "pointers": { "price": "/p" } }))]));
    assert!(added.unwrap().refused_over_mcp.is_empty(), "a new service with no key changes no secret");
}

#[test]
fn the_schema_declares_a_default_and_description_for_every_setting() {
    fn walk(schema: &Value, at: &str, missing: &mut Vec<String>) {
        if let Some(props) = properties(schema) {
            props.iter().for_each(|(key, s)| walk(s, &join(at, key), missing));
            return;
        }
        if schema.get("description").is_none() || (schema.get("default").is_none() && !at.contains("[entry]")) {
            missing.push(at.to_owned());
        }
        if let Some(entry) = entries(schema) {
            properties(entry).into_iter().flatten().filter(|(_, s)| s.get("description").is_none()).for_each(|(k, _)| missing.push(format!("{at}.{k}")));
        }
    }
    let mut missing = Vec::new();
    walk(&SCHEMA, "", &mut missing);
    assert_eq!(missing, Vec::<String>::new());
    assert!(Config::default().problems().is_empty(), "the defaults are valid: {:?}", Config::default().problems());
}

#[test]
fn any_url_of_a_service_binds_its_key_and_key_variable() {
    let mut c = config(r#"{"quotes": {"services": {"finnhub": {"key": "F", "keyEnv": "MY_FINNHUB"}}}}"#);
    let same = c.apply(&change(&[("quotes.services.finnhub.currencyUrl", json!("https://finnhub.io/api/v1/forex?b={base}&token={key}"))]));
    assert!(same.unwrap().cleared_keys.is_empty(), "a new URL on a host the service already uses");
    let added = c.apply(&change(&[("quotes.services.finnhub.currencyUrl", json!("https://rates.example/{base}/{quote}"))])).unwrap();
    assert_eq!(added.cleared_keys, ["quotes.services.finnhub.key", "quotes.services.finnhub.keyEnv"]);
    assert_eq!(added.refused_over_mcp, Vec::<String>::new());
    assert_eq!(c.setting(&path("quotes.services.finnhub.keyEnv")).unwrap(), "FINNHUB_API_KEY", "the default variable shows again");
    let env = c.apply(&change(&[("quotes.services.finnhub.keyEnv", json!("OTHER_KEY"))])).unwrap();
    assert_eq!(env.refused_over_mcp, ["quotes.services.finnhub.keyEnv"], "naming a variable hands a key on");
}
