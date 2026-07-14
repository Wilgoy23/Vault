//! Parses password CSV exports into vault entries. Column mapping is
//! header-based and case-insensitive, so Chrome/Edge, Firefox, Bitwarden,
//! LastPass and anything else with recognizable headers all work without
//! the user having to pick a format.

use zeroize::{Zeroize, ZeroizeOnDrop};

/// One login parsed from a CSV row, before it becomes a vault Entry.
/// Zeroized on drop — rows hold plaintext passwords.
#[derive(Debug, Default, Zeroize, ZeroizeOnDrop)]
pub struct CsvLogin {
    pub name: String,
    pub username: Option<String>,
    pub email: String,
    pub password: String,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub totp_secret: Option<String>,
    pub folder: Option<String>,
}

#[derive(Debug)]
pub struct CsvParseResult {
    pub logins: Vec<CsvLogin>,
    /// Rows that were not importable logins (secure notes, empty passwords, …)
    pub skipped: usize,
}

/// Finds the index of the first header matching any of the candidate names.
fn find_col(headers: &[String], candidates: &[&str]) -> Option<usize> {
    candidates
        .iter()
        .find_map(|c| headers.iter().position(|h| h == c))
}

/// "https://www.example.com/login" → "example.com"
fn domain_from_url(url: &str) -> Option<String> {
    let no_scheme = url.split("://").nth(1).unwrap_or(url);
    let host = no_scheme.split(['/', '?', '#']).next()?;
    let host = host.split('@').last()?.split(':').next()?;
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.is_empty() { None } else { Some(host.to_string()) }
}

/// Pulls the secret out of an otpauth:// URI; anything else passes through
/// unchanged (assumed to already be a raw Base32 secret).
fn normalize_totp(value: &str) -> String {
    if !value.to_lowercase().starts_with("otpauth://") {
        return value.to_string();
    }
    value
        .split(['?', '&'])
        .find_map(|p| p.strip_prefix("secret=").or_else(|| p.strip_prefix("SECRET=")))
        .unwrap_or(value)
        .to_string()
}

/// Parses CSV text into logins. Errors when the headers don't look like a
/// password export at all (no password column).
pub fn parse(content: &str) -> Result<CsvParseResult, String> {
    let content = content.trim_start_matches('\u{feff}');

    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(content.as_bytes());

    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| format!("Could not read CSV headers: {e}"))?
        .iter()
        .map(|h| h.trim().to_lowercase())
        .collect();

    let password_col = find_col(&headers, &["password", "login_password", "pass"])
        .ok_or("Unrecognized CSV format — no password column found")?;
    let name_col = find_col(&headers, &["name", "title", "account"]);
    let url_col = find_col(&headers, &["url", "login_uri", "website", "web site", "uri", "origin"]);
    let username_col = find_col(&headers, &["username", "login_username", "user"]);
    let notes_col = find_col(&headers, &["notes", "note", "extra", "comments"]);
    let totp_col = find_col(&headers, &["totp", "login_totp", "otp"]);
    let folder_col = find_col(&headers, &["folder", "grouping", "group"]);
    // Bitwarden exports mix logins with secure notes; the type column tells them apart
    let type_col = find_col(&headers, &["type"]);

    let get = |record: &csv::StringRecord, col: Option<usize>| -> String {
        col.and_then(|i| record.get(i)).unwrap_or("").trim().to_string()
    };
    let opt = |s: String| if s.is_empty() { None } else { Some(s) };

    let mut logins = Vec::new();
    let mut skipped = 0usize;

    for record in reader.records() {
        let record = record.map_err(|e| format!("Malformed CSV row: {e}"))?;

        let row_type = get(&record, type_col);
        if type_col.is_some() && !row_type.is_empty() && row_type.to_lowercase() != "login" {
            skipped += 1;
            continue;
        }

        let mut password = get(&record, Some(password_col));
        if password.is_empty() {
            skipped += 1;
            continue;
        }

        let mut user = get(&record, username_col);
        let url = get(&record, url_col);

        // The vault model has separate email and username fields; exports
        // only have "username", which is usually an email address
        let (email, username) = if user.contains('@') {
            (std::mem::take(&mut user), None)
        } else {
            (String::new(), opt(std::mem::take(&mut user)))
        };

        let name = {
            let explicit = get(&record, name_col);
            if !explicit.is_empty() {
                explicit
            } else if let Some(domain) = domain_from_url(&url) {
                domain
            } else if !email.is_empty() {
                email.clone()
            } else if let Some(u) = &username {
                u.clone()
            } else {
                "Imported entry".to_string()
            }
        };

        logins.push(CsvLogin {
            name,
            username,
            email,
            password: std::mem::take(&mut password),
            url: opt(url),
            notes: opt(get(&record, notes_col)),
            totp_secret: opt(get(&record, totp_col)).map(|t| normalize_totp(&t)),
            folder: opt(get(&record, folder_col)),
        });
    }

    Ok(CsvParseResult { logins, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chrome_export() {
        let csv = "name,url,username,password,note\n\
                   GitHub,https://github.com/login,octo@example.com,hunter2,my note\n\
                   Router,http://192.168.1.1,admin,letmein,\n";
        let result = parse(csv).unwrap();
        assert_eq!(result.logins.len(), 2);
        assert_eq!(result.skipped, 0);

        let gh = &result.logins[0];
        assert_eq!(gh.name, "GitHub");
        assert_eq!(gh.email, "octo@example.com");
        assert_eq!(gh.username, None);
        assert_eq!(gh.password, "hunter2");
        assert_eq!(gh.url.as_deref(), Some("https://github.com/login"));
        assert_eq!(gh.notes.as_deref(), Some("my note"));

        let router = &result.logins[1];
        assert_eq!(router.email, "");
        assert_eq!(router.username.as_deref(), Some("admin"));
    }

    #[test]
    fn parses_firefox_export_deriving_name_from_url() {
        let csv = "\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\
                   \"https://www.reddit.com\",\"snoo\",\"upvote123\",\"\",\"https://www.reddit.com\",\"{abc}\",\"1\",\"2\",\"3\"\n";
        let result = parse(csv).unwrap();
        assert_eq!(result.logins.len(), 1);
        assert_eq!(result.logins[0].name, "reddit.com");
        assert_eq!(result.logins[0].username.as_deref(), Some("snoo"));
        assert_eq!(result.logins[0].password, "upvote123");
    }

    #[test]
    fn parses_bitwarden_export_skipping_notes_and_extracting_totp() {
        let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                   Work,,login,Jira,,,0,https://jira.example.com,dev@example.com,pw1,otpauth://totp/Jira:dev?secret=JBSWY3DPEHPK3PXP&issuer=Jira\n\
                   ,,note,My Secure Note,some text,,0,,,,\n";
        let result = parse(csv).unwrap();
        assert_eq!(result.logins.len(), 1);
        assert_eq!(result.skipped, 1);

        let jira = &result.logins[0];
        assert_eq!(jira.name, "Jira");
        assert_eq!(jira.folder.as_deref(), Some("Work"));
        assert_eq!(jira.totp_secret.as_deref(), Some("JBSWY3DPEHPK3PXP"));
    }

    #[test]
    fn parses_lastpass_export_with_grouping() {
        let csv = "url,username,password,totp,extra,name,grouping,fav\n\
                   https://n26.com,bank@example.com,pw2,JBSWY3DPEHPK3PXP,note here,N26,Finance,0\n";
        let result = parse(csv).unwrap();
        assert_eq!(result.logins.len(), 1);
        let n26 = &result.logins[0];
        assert_eq!(n26.name, "N26");
        assert_eq!(n26.folder.as_deref(), Some("Finance"));
        assert_eq!(n26.totp_secret.as_deref(), Some("JBSWY3DPEHPK3PXP"));
        assert_eq!(n26.notes.as_deref(), Some("note here"));
    }

    #[test]
    fn skips_rows_without_password() {
        let csv = "name,url,username,password\n\
                   Empty,https://a.com,user,\n\
                   Kept,https://b.com,user,pw\n";
        let result = parse(csv).unwrap();
        assert_eq!(result.logins.len(), 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.logins[0].name, "Kept");
    }

    #[test]
    fn rejects_csv_without_password_column() {
        let csv = "first_name,last_name,phone\nJohn,Doe,555-1234\n";
        let result = parse(csv);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no password column"));
    }

    #[test]
    fn strips_utf8_bom_before_reading_headers() {
        let csv = "\u{feff}name,url,username,password\nSite,https://s.com,u@e.com,pw\n";
        let result = parse(csv).unwrap();
        assert_eq!(result.logins.len(), 1);
        assert_eq!(result.logins[0].name, "Site");
    }

    #[test]
    fn domain_extraction_handles_ports_paths_and_bare_hosts() {
        assert_eq!(domain_from_url("https://www.example.com/a/b"), Some("example.com".into()));
        assert_eq!(domain_from_url("http://192.168.1.1:8080"), Some("192.168.1.1".into()));
        assert_eq!(domain_from_url("example.org"), Some("example.org".into()));
        assert_eq!(domain_from_url(""), None);
    }
}
