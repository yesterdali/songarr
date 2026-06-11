//! R2 voting/ranking. Sources can be individually flaky; this layer turns
//! whatever came back into one deterministic candidate list.

use std::collections::{HashMap, HashSet};

use super::RecCandidate;

#[derive(Debug, Clone)]
pub struct SourceCandidates {
    pub source: &'static str,
    pub weight: f32,
    pub candidates: Vec<RecCandidate>,
}

#[derive(Debug)]
struct Merged {
    candidate: RecCandidate,
    score: f32,
    sources: HashSet<&'static str>,
    best_rank: usize,
}

pub fn merge_sources(sources: Vec<SourceCandidates>, limit: usize) -> Vec<RecCandidate> {
    if limit == 0 {
        return Vec::new();
    }

    let mut merged: HashMap<String, Merged> = HashMap::new();
    for source in sources.into_iter().filter(|s| s.weight > 0.0) {
        // Clean and filter before keying so cruft variants ("Tell Me (Official
        // Video)") vote together with the clean track, and alternate edits
        // (slowed/reverb/parody) never compete for slots at all.
        let cleaned: Vec<RecCandidate> = source
            .candidates
            .into_iter()
            .filter(|c| !super::is_junk_version(&c.title))
            .map(|mut c| {
                c.title = super::clean_title(&c.title);
                c.title = super::strip_artist_prefix(&c.title, &c.artist);
                c
            })
            .filter(|c| !c.title.trim().is_empty())
            .collect();
        for (rank, candidate) in cleaned.into_iter().enumerate() {
            let key = candidate.song_key();
            if key == "|" {
                continue;
            }
            let rank_score = source.weight / ((rank + 1) as f32).sqrt();
            let entry = merged.entry(key).or_insert_with(|| Merged {
                candidate: candidate.clone(),
                score: 0.0,
                sources: HashSet::new(),
                best_rank: rank,
            });
            entry.score += rank_score;
            entry.best_rank = entry.best_rank.min(rank);
            entry.sources.insert(source.source);
            if better_metadata(&candidate, &entry.candidate) {
                entry.candidate = candidate;
            }
        }
    }

    let mut values: Vec<_> = merged.into_values().collect();
    for value in &mut values {
        // Make agreement matter: two modest votes should outrank one merely
        // good rank, which is the core R2 behavior.
        value.score += (value.sources.len().saturating_sub(1) as f32) * 0.75;
    }
    values.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.best_rank.cmp(&b.best_rank))
            .then_with(|| a.candidate.artist.cmp(&b.candidate.artist))
            .then_with(|| a.candidate.title.cmp(&b.candidate.title))
    });
    values
        .into_iter()
        .take(limit)
        .map(|merged| merged.candidate)
        .collect()
}

fn better_metadata(new: &RecCandidate, old: &RecCandidate) -> bool {
    let new_score = metadata_score(new);
    let old_score = metadata_score(old);
    new_score > old_score
}

fn metadata_score(candidate: &RecCandidate) -> u32 {
    u32::from(candidate.video_id.is_some()) * 8
        + u32::from(candidate.provider.as_deref() == Some("deezer")) * 4
        + u32::from(candidate.artwork_url.is_some()) * 2
        + u32::from(candidate.duration_ms.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(artist: &str, title: &str) -> RecCandidate {
        RecCandidate {
            artist: artist.into(),
            title: title.into(),
            album: None,
            duration_ms: Some(180_000),
            isrc: None,
            artwork_url: None,
            provider: None,
            provider_track_id: None,
            video_id: None,
        }
    }

    #[test]
    fn overlapping_candidate_beats_single_source_first_place() {
        let merged = merge_sources(
            vec![
                SourceCandidates {
                    source: "ytm",
                    weight: 1.0,
                    candidates: vec![c("A", "Flashy"), c("B", "Consensus")],
                },
                SourceCandidates {
                    source: "deezer",
                    weight: 0.8,
                    candidates: vec![c("B", "Consensus")],
                },
            ],
            5,
        );
        assert_eq!(merged[0].title, "Consensus");
    }

    #[test]
    fn weights_can_disable_sources() {
        let merged = merge_sources(
            vec![
                SourceCandidates {
                    source: "ytm",
                    weight: 0.0,
                    candidates: vec![c("A", "Disabled")],
                },
                SourceCandidates {
                    source: "deezer",
                    weight: 1.0,
                    candidates: vec![c("B", "Enabled")],
                },
            ],
            5,
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].title, "Enabled");
    }

    #[test]
    fn richer_metadata_survives_merge() {
        let mut rich = c("B", "Consensus");
        rich.provider = Some("deezer".into());
        rich.provider_track_id = Some("42".into());
        rich.artwork_url = Some("https://example.com/c.jpg".into());
        let mut fast = c("B", "Consensus");
        fast.video_id = Some("vid".into());
        let merged = merge_sources(
            vec![
                SourceCandidates {
                    source: "deezer",
                    weight: 1.0,
                    candidates: vec![rich],
                },
                SourceCandidates {
                    source: "ytm",
                    weight: 1.0,
                    candidates: vec![fast],
                },
            ],
            5,
        );
        assert_eq!(merged[0].video_id.as_deref(), Some("vid"));
    }
}
