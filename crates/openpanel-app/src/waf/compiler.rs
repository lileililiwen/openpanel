//! Pure typed-rule to nginx-snippet compiler.

use openpanel_domain::waf::{DefaultAction, HeaderChallenge, Rule, RuleAction, RuleSet, WafError};

/// Deterministic compiler for validated WAF rule sets.
pub struct NginxSnippetCompiler;

impl NginxSnippetCompiler {
    /// Compile enabled rules in stable priority/id order.
    pub fn compile(ruleset: &RuleSet) -> Result<String, WafError> {
        if ruleset.rules().is_empty() {
            return Ok(String::new());
        }
        for rule in ruleset.rules() {
            rule.validate()?;
        }
        let site = ruleset.site_id().simple().to_string();
        let site_short = &site[..12];
        let mut output = format!(
            "# openpanel-waf site={} rev={}\n",
            ruleset.site_id(),
            ruleset.version()
        );
        for rule in ruleset.rules().iter().filter(|rule| rule.enabled()) {
            output.push_str(&format!("# rule={} kind={}\n", rule.id(), rule.kind()));
            output.push_str(&compile_rule(rule, site_short));
        }
        match ruleset.default_action() {
            DefaultAction::Allow => {}
            DefaultAction::Challenge => output.push_str("return 302 /openpanel-waf-challenge;\n"),
            DefaultAction::Deny => output.push_str("return 403;\n"),
        }
        Ok(output)
    }

    /// Compile one rule for the dry-run endpoint.
    pub fn compile_one(site_id: uuid::Uuid, rule: Rule) -> Result<String, WafError> {
        let set = RuleSet::new(site_id, 1, DefaultAction::Allow, vec![rule])?;
        Self::compile(&set)
    }
}

fn compile_rule(rule: &Rule, site: &str) -> String {
    match rule {
        Rule::RateLimit {
            zone,
            rate,
            burst,
            nodelay,
            ..
        } => format!(
            "limit_req_zone $binary_remote_addr zone=op_{site}_{zone}:10m rate={rate}r/s;\nlimit_req zone=op_{site}_{zone} burst={burst}{};\nlimit_req_status 429;\n",
            if *nodelay { " nodelay" } else { "" }
        ),
        Rule::ConnLimit { zone, per_ip, .. } => {
            format!(
                "limit_conn_zone $binary_remote_addr zone=op_{site}_{zone}:10m;\nlimit_conn op_{site}_{zone} {per_ip};\nlimit_conn_status 429;\n"
            )
        }
        Rule::GeoBlock {
            countries, action, ..
        } => format!(
            "if ($http_cf_ipcountry ~* \"^({})$\") {{ {} }}\n",
            countries.join("|"),
            action_directive(*action)
        ),
        Rule::UserAgentBlock {
            pattern, action, ..
        } => format!(
            "if ($http_user_agent ~* \"{pattern}\") {{ {} }}\n",
            action_directive(*action)
        ),
        Rule::PathBlock {
            pattern,
            method,
            action,
            ..
        } => match method {
            Some(method) => format!(
                "location ^~ {pattern} {{ if ($request_method = {method}) {{ {} }} }}\n",
                action_directive(*action)
            ),
            None => format!(
                "location ^~ {pattern} {{ {} }}\n",
                action_directive(*action)
            ),
        },
        Rule::HeaderChallenge {
            header,
            value,
            challenge,
            ..
        } => {
            let variable = header.to_ascii_lowercase().replace('-', "_");
            let directive = match challenge {
                HeaderChallenge::Tarpit => "return 429;".to_owned(),
                HeaderChallenge::Redirect { path } => format!("return 302 {path};"),
            };
            format!("if ($http_{variable} = \"{value}\") {{ {directive} }}\n")
        }
        Rule::BodySizeCap { max_bytes, .. } => {
            format!("client_max_body_size {max_bytes};\n")
        }
    }
}

fn action_directive(action: RuleAction) -> &'static str {
    match action {
        RuleAction::Allow => "set $openpanel_waf_allowed 1;",
        RuleAction::Challenge => "return 302 /openpanel-waf-challenge;",
        RuleAction::Deny => "return 403;",
    }
}
