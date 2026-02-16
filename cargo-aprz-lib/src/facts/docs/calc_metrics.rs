//! Metrics calculation for rustdoc JSON documentation
//!
//! This module handles parsing rustdoc JSON in various format versions and extracting
//! documentation metrics.

use super::provider::LOG_TARGET;
use super::{DocsData, DocsMetrics};
use crate::Result;
use crate::facts::CrateSpec;
use ohno::{IntoAppError, app_err};
use regex::Regex;
use crate::HashMap;
use std::io::Read;
use std::sync::LazyLock;

/// Pattern to match intra-doc code links: [`text`]
/// Only matches backtick-enclosed links which are the standard for code references
static INTRA_DOC_LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[`([^`\]]+)`\]").expect("invalid regex"));

/// Pattern to match code blocks (triple backticks)
static CODE_BLOCK_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"```[\s\S]*?```").expect("invalid regex"));

/// Pattern to match reference-style link definitions: [`text`]: target
/// These define aliases where the link text in the docs maps to a different resolution target
static LINK_REFERENCE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[`([^`\]]+)`\]:\s*(\S+)").expect("invalid regex"));

/// Macro to generate all code needed for a rustdoc JSON format version
///
/// This generates both:
/// 1. The version-specific `calculate_metrics_vN` function
/// 2. The `ItemLike` trait implementation for that version's types
macro_rules! generate_version_support {
    ($version:literal, $module:ident) => {
        pastey::paste! {
            /// Parse and calculate metrics for rustdoc JSON format version
            #[doc = $version]
            fn [<calculate_metrics_v $version>](json_value: serde_json::Value, crate_spec: &CrateSpec) -> Result<DocsMetrics> {
                use $module as rustdoc_types;

                log::debug!(target: LOG_TARGET, "Parsing rustdoc JSON v{} for {crate_spec}", $version);
                let krate: rustdoc_types::Crate = serde_json::from_value(json_value)
                    .into_app_err_with(|| format!("parsing rustdoc JSON v{} structure for {crate_spec}", $version))?;

                let index_len = krate.index.len();
                log::debug!(target: LOG_TARGET, "Successfully parsed rustdoc JSON v{} for {crate_spec}, found {index_len} items in index", $version);
                log::debug!(target: LOG_TARGET, "Root item ID for {crate_spec}: {:?}", krate.root);

                Ok(process_crate_items(
                    &krate.index,
                    &krate.root,
                    crate_spec,
                    |item| matches!(item.visibility, rustdoc_types::Visibility::Public),
                    |item| matches!(item.inner, rustdoc_types::ItemEnum::Use(_)),
                ))
            }
        }

        // Generate ItemLike trait implementation
        impl ItemLike for $module::Item {
            type Id = $module::Id;

            fn name(&self) -> Option<&str> {
                self.name.as_deref()
            }

            fn docs(&self) -> Option<&str> {
                self.docs.as_deref()
            }

            fn links(&self) -> &std::collections::HashMap<String, Self::Id> {
                &self.links
            }
        }
    };
}

// Generate all code for each supported version
generate_version_support!("50", rustdoc_types_v50);
generate_version_support!("51", rustdoc_types_v51);
generate_version_support!("52", rustdoc_types_v52);
generate_version_support!("53", rustdoc_types_v53);
generate_version_support!("54", rustdoc_types_v54);
generate_version_support!("55", rustdoc_types_v55);
generate_version_support!("56", rustdoc_types_v56);
generate_version_support!("57", rustdoc_types_v57);

pub fn calculate_docs_metrics(reader: impl Read, crate_spec: &CrateSpec) -> Result<DocsData> {
    log::debug!(target: LOG_TARGET, "Parsing rustdoc JSON for {crate_spec}");
    let json_value: serde_json::Value =
        serde_json::from_reader(reader).into_app_err_with(|| format!("parsing JSON for {crate_spec}"))?;

    let format_version = json_value
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| app_err!("rustdoc JSON for {crate_spec} is missing 'format_version' field"))?;

    log::debug!(target: LOG_TARGET, "Found rustdoc JSON format version {format_version} for {crate_spec}");

    let metrics = match format_version {
        50 => calculate_metrics_v50(json_value, crate_spec)?,
        51 => calculate_metrics_v51(json_value, crate_spec)?,
        52 => calculate_metrics_v52(json_value, crate_spec)?,
        53 => calculate_metrics_v53(json_value, crate_spec)?,
        54 => calculate_metrics_v54(json_value, crate_spec)?,
        55 => calculate_metrics_v55(json_value, crate_spec)?,
        56 => calculate_metrics_v56(json_value, crate_spec)?,
        57 => calculate_metrics_v57(json_value, crate_spec)?,
        _ => {
            log::debug!(target: LOG_TARGET, "Unsupported rustdoc JSON format version {format_version} for {crate_spec}");
            return Err(app_err!(
                "unsupported rustdoc JSON format version {format_version} for {crate_spec}"
            ));
        }
    };

    Ok(DocsData {
        metrics,
    })
}

/// Process crate items and calculate documentation metrics
///
/// This generic function works with items from any rustdoc-types version by accepting
/// closures that check visibility and item type in a version-specific way.
fn process_crate_items<Id, Item>(
    index: &std::collections::HashMap<Id, Item>,
    root_id: &Id,
    crate_spec: &CrateSpec,
    is_public: impl Fn(&Item) -> bool,
    is_use_item: impl Fn(&Item) -> bool,
) -> DocsMetrics
where
    Id: core::fmt::Debug + Eq + core::hash::Hash,
    Item: ItemLike,
{
    let mut number_of_public_api_elements = 0;
    let mut documented_count = 0;
    let mut number_of_examples_in_docs = 0;
    let mut has_crate_level_docs = false;
    let mut broken_doc_links = 0;
    let mut private_items = 0;
    let mut use_items = 0;

    let index_len = index.len();
    log::debug!(target: LOG_TARGET, "Starting to iterate through {index_len} items for {crate_spec}");

    for (id, item) in index {
        // Only count public API items
        if !is_public(item) {
            private_items += 1;
            continue;
        }

        // Skip re-exports (Use items) - they inherit docs from the original item
        if is_use_item(item) {
            use_items += 1;
            continue;
        }

        number_of_public_api_elements += 1;

        // Check if item has documentation
        if let Some(docs) = item.docs()
            && !docs.trim().is_empty()
        {
            documented_count += 1;

            let fences = docs.lines().filter(|line| line.trim_start().starts_with("```")).count();
            let examples = fences / 2; // Divide by 2 since each codebase block has opening and closing fence
            number_of_examples_in_docs += examples;

            let broken = count_broken_links::<Item::Id>(docs, item.links(), item.name());
            broken_doc_links += broken;

            if let Some(name) = item.name()
                && name == crate_spec.name()
                && id == root_id
            {
                log::debug!(target: LOG_TARGET, "Found crate-level docs for {crate_spec} (root item name matches)");
                has_crate_level_docs = true;
            }
        }
    }

    log::debug!(target: LOG_TARGET, "Iteration complete for {crate_spec}: processed {index_len} items (private={private_items}, use_items={use_items}, public_api={number_of_public_api_elements})");

    log::debug!(target: LOG_TARGET, "Finished processing items for {crate_spec}: public_api={number_of_public_api_elements}, documented={documented_count}, examples={number_of_examples_in_docs}, broken_links={broken_doc_links}, has_crate_docs={has_crate_level_docs}");

    #[expect(clippy::cast_precision_loss, reason = "loss of precision acceptable for percentage calculation")]
    let doc_coverage_percentage = if number_of_public_api_elements > 0 {
        documented_count as f64 / number_of_public_api_elements as f64 * 100.0
    } else {
        100.0
    };

    log::debug!(target: LOG_TARGET, "Calculated coverage percentage for {crate_spec}: {doc_coverage_percentage}%");

    let metrics = DocsMetrics {
        doc_coverage_percentage,
        public_api_elements: number_of_public_api_elements,
        undocumented_elements: number_of_public_api_elements - documented_count,
        examples_in_docs: number_of_examples_in_docs as u64,
        has_crate_level_docs,
        broken_doc_links,
    };

    log::debug!(target: LOG_TARGET, "Returning DocsMetrics for {crate_spec}: {metrics:?}");
    metrics
}

/// Count broken intra-doc links in documentation
///
/// Looks for markdown link patterns that appear to be intra-doc links but aren't
/// in the resolved links map. Only considers backtick-enclosed links like [`Type`]
/// which are the standard way to reference code elements in Rust documentation.
///
/// Handles reference-style link definitions where the link text in the docs
/// (e.g., `` [`anyhow::Error::from_boxed`] ``) is defined to resolve to a different target
/// (e.g., `Self::from_boxed`) via a line like: `` [`anyhow::Error::from_boxed`]: Self::from_boxed ``
fn count_broken_links<Id>(docs: &str, resolved_links: &std::collections::HashMap<String, Id>, _item_name: Option<&str>) -> u64 {
    let mut broken_count = 0;
    let mut skipped_inline = 0;
    let mut skipped_external = 0;
    let mut skipped_short = 0;
    let mut skipped_resolved = 0;

    log::trace!(target: LOG_TARGET, "Checking for broken links. Docs length: {} chars, resolved_links count: {}", docs.len(), resolved_links.len());

    // Remove code blocks to avoid false positives from examples
    let docs_without_code_blocks = CODE_BLOCK_REGEX.replace_all(docs, "");
    let docs_to_check = docs_without_code_blocks.as_ref();

    // Parse reference-style link definitions: [`link_text`]: target
    // These map the link text as written in the docs to the actual resolution target
    let mut link_references = HashMap::default();
    for cap in LINK_REFERENCE_REGEX.captures_iter(docs_to_check) {
        if let (Some(link_text), Some(target)) = (cap.get(1), cap.get(2)) {
            let _ = link_references.insert(link_text.as_str(), target.as_str());
            log::trace!(target: LOG_TARGET, "Found link reference: [`{}`] -> {}", link_text.as_str(), target.as_str());
        }
    }

    for cap in INTRA_DOC_LINK_REGEX.captures_iter(docs_to_check) {
        if let Some(link_text) = cap.get(1) {
            let text = link_text.as_str();

            // Get the position after the match to check for inline link syntax
            let match_end = cap.get(0).expect("match exists").end();

            // Skip inline links like [`text`](url) - check if next char is '('
            if docs_to_check.get(match_end..).is_some_and(|s| s.starts_with('(')) {
                skipped_inline += 1;
                log::trace!(target: LOG_TARGET, "Skipping inline link: [`{text}`](...)");
                continue;
            }

            // Check for inline reference-style links like [`text`][target]
            // Extract the target if present (it's in square brackets but WITHOUT backticks)
            let inline_target = (|| {
                let remainder = docs_to_check.get(match_end..)?.strip_prefix('[')?;
                let end_pos = remainder.find(']')?;
                remainder.get(..end_pos)
            })();

            // Skip external links (contain ://)
            if text.contains("://") {
                skipped_external += 1;
                log::trace!(target: LOG_TARGET, "Skipping external link: [`{text}`]");
                continue;
            }

            // Skip very short "links" (1-2 chars) - likely false positives
            let text_len = text.len();
            if text_len <= 2 {
                skipped_short += 1;
                log::trace!(target: LOG_TARGET, "Skipping short link (len={text_len}): [`{text}`]");
                continue;
            }

            // Check if it's resolved - try multiple strategies:
            // 1. Direct match in resolved_links (with and without backticks)
            // 2. Via an inline reference target [`text`][target]
            // 3. Via a reference definition (link text -> target, then check if target is in resolved_links)
            // 4. Strip trailing () for method references and try again
            // 5. Try without module path if it contains ::

            // Some links maps include backticks as part of the key (e.g., "`Error::chain`")
            let text_with_backticks = format!("`{text}`");

            // Strip trailing () from method references like `chain()`
            let text_without_parens = text.strip_suffix("()").unwrap_or(text);
            let text_without_parens_with_backticks = format!("`{text_without_parens}`");

            let is_resolved = resolved_links.contains_key(text)
                || resolved_links.contains_key(text_with_backticks.as_str())
                || resolved_links.contains_key(text_without_parens)
                || resolved_links.contains_key(text_without_parens_with_backticks.as_str())
                || inline_target.is_some_and(|target| resolved_links.contains_key(target))
                || link_references.get(text).is_some_and(|target| resolved_links.contains_key(*target))
                || link_references
                    .get(text_without_parens)
                    .is_some_and(|target| resolved_links.contains_key(*target))
                || (text_without_parens.contains("::") && {
                    // Try just the last component (e.g., "Error" from "std::error::Error", or "chain" from "Error::chain")
                    let last_component = text_without_parens.rsplit("::").next().unwrap_or("");
                    resolved_links.contains_key(last_component)
                        || link_references
                            .get(last_component)
                            .is_some_and(|target| resolved_links.contains_key(*target))
                });

            if is_resolved {
                skipped_resolved += 1;
                log::trace!(target: LOG_TARGET, "Resolved link: [`{text}`]");
                continue;
            }

            // This looks like an intra-doc link but isn't resolved
            broken_count += 1;
            log::trace!(target: LOG_TARGET, "Broken link: [`{text}`]");
        }
    }

    let total_matches = broken_count + skipped_inline + skipped_external + skipped_short + skipped_resolved;
    log::trace!(target: LOG_TARGET, "Link analysis: total_matches={total_matches}, broken={broken_count}, skipped(inline={skipped_inline}, external={skipped_external}, short={skipped_short}, resolved={skipped_resolved})");

    broken_count
}

/// Trait to abstract over different rustdoc-types Item versions
trait ItemLike {
    type Id;
    fn name(&self) -> Option<&str>;
    fn docs(&self) -> Option<&str>;
    fn links(&self) -> &std::collections::HashMap<String, Self::Id>;
}
