//! The operator search (F3), as in Blender: every operator, ranked by how well its name matches,
//! with the menu it lives in and its shortcut.

use crate::app::AzamApp;
use crate::operators::{Menu, MenuEntry, Operator};

/// How many results the search lists.
pub const SEARCH_LIMIT: usize = 14;

/// Where `op` lives in the top-bar menus, as Blender shows it ("File ▸ New"), if anywhere.
#[must_use]
pub fn menu_path(menus: &[Menu], op: Operator) -> Option<String> {
    menus.iter().find_map(|m| {
        m.entries.iter().find_map(|e| match e {
            MenuEntry::Item(o) if *o == op => Some(m.title.to_owned()),
            MenuEntry::Sub(title, _, items) if items.contains(&op) => Some(format!("{} ▸ {title}", m.title)),
            _ => None,
        })
    })
}

/// How well `label` (in menu `path`) matches the lowercased words of `query`, lower is better;
/// `None` if some word matches neither. A name starting with the query ranks first, then names
/// with a word starting with each query word, then names containing them, then menu matches.
#[must_use]
pub fn rank(label: &str, path: Option<&str>, query: &str) -> Option<u32> {
    let label = label.to_lowercase();
    let path = path.unwrap_or("").to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return Some(0);
    }
    let label_words: Vec<&str> = label
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut score = 0;
    for w in &words {
        score += if label_words.iter().any(|lw| lw.starts_with(w)) {
            1
        } else if label.contains(w) {
            2
        } else if path.contains(w) {
            4
        } else {
            return None;
        };
    }
    Some(if label.starts_with(query.trim()) { 0 } else { score })
}

impl AzamApp {
    /// The operators matching the search query, best first, with their menu paths.
    pub(crate) fn search_results(&self) -> Vec<(Operator, Option<String>)> {
        let query = self.search.as_deref().unwrap_or("").to_lowercase();
        let top = &self.menus[..self.top_menu_count()];
        let mut hits: Vec<(u32, usize, Operator, Option<String>)> = Operator::searchable()
            .into_iter()
            .enumerate()
            .filter_map(|(i, op)| {
                let path = menu_path(top, op);
                rank(&op.label(), path.as_deref(), &query).map(|r| (r, i, op, path))
            })
            .collect();
        // with nothing typed, the operators run lately come first, newest first
        if query.trim().is_empty() {
            for h in &mut hits {
                if let Some(k) = self.recent_ops.iter().position(|o| *o == h.2) {
                    h.0 = 0;
                    h.1 = k;
                } else {
                    h.0 = 1;
                }
            }
        }
        hits.sort_by_key(|&(r, i, ..)| (r, i));
        hits.into_iter()
            .take(SEARCH_LIMIT)
            .map(|(_, _, op, path)| (op, path))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_that_start_with_the_query_come_first() {
        let fit = rank("Fit Map Shading", Some("View"), "fi").expect("match");
        let file = rank("Share via File Log", Some("File"), "fi").expect("match");
        assert!(
            fit < file,
            "a name starting with 'fi' beats one with a later word"
        );
        assert!(
            rank("Export DXF…", Some("File"), "file export").is_some(),
            "words may match the menu"
        );
        assert!(rank("Export DXF…", Some("File"), "render").is_none());
        assert_eq!(rank("Undo", Some("Edit"), ""), Some(0));
    }

    #[test]
    fn operators_know_their_menus() {
        let menus = crate::operators::top_menus();
        assert_eq!(menu_path(&menus, Operator::Save).as_deref(), Some("File"));
        assert_eq!(
            menu_path(&menus, Operator::New(crate::operators::Template::Dress)).as_deref(),
            Some("File ▸ New")
        );
    }
}
