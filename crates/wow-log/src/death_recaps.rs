use crate::Event;
use owc_domain::DeathHit;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeathRecaps {
    players: BTreeMap<String, Vec<(i64, DeathHit)>>,
}
impl DeathRecaps {
    pub fn ingest(&mut self, event: &Event) -> Vec<DeathHit> {
        if matches!(event.kind(), "ZONE_CHANGE" | "COMBAT_LOG_VERSION") {
            self.players.clear();
        }
        self.players.retain(|_, hits| {
            hits.retain(|(at, _)| event.at.saturating_sub(*at) <= 10_000 && *at <= event.at);
            !hits.is_empty()
        });
        let target = event.field(5);
        if !target.starts_with("Player-") {
            return vec![];
        }
        if event.kind() == "UNIT_DIED" {
            let hits = self.players.remove(target).unwrap_or_default();
            if event.field(9) == "1" {
                return vec![];
            }
            return hits
                .into_iter()
                .map(|(at, mut hit)| {
                    hit.offset_ms = at - event.at;
                    hit
                })
                .collect();
        }
        if event.kind() == "SPELL_RESURRECT" {
            self.players.remove(target);
            return vec![];
        }
        let (ability, spell_id) = match event.kind() {
            "SWING_DAMAGE" => ("Melee".to_owned(), None),
            "ENVIRONMENTAL_DAMAGE" => (event.field(9).to_lowercase(), None),
            "SPELL_DAMAGE"
            | "SPELL_PERIODIC_DAMAGE"
            | "RANGE_DAMAGE"
            | "DAMAGE_SHIELD"
            | "DAMAGE_SPLIT" => {
                let Some(id) = event.number(9) else {
                    return vec![];
                };
                if event.field(10).is_empty() {
                    return vec![];
                }
                (event.field(10).to_owned(), Some(id))
            }
            _ => return vec![],
        };
        // The cap also bounds malformed logs containing an unlimited stream of unique GUIDs.
        if self.players.len() >= 128 && !self.players.contains_key(target) {
            return vec![];
        }
        let hits = self.players.entry(target.to_owned()).or_default();
        hits.push((
            event.at,
            DeathHit {
                offset_ms: 0,
                source: if event.kind() == "ENVIRONMENTAL_DAMAGE" {
                    "Environment".into()
                } else {
                    event.field(2).into()
                },
                ability,
                spell_id,
            },
        ));
        if hits.len() > 3 {
            hits.remove(0);
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(at: i64, kind: &str, player: &str) -> Event {
        Event {
            at,
            fields: crate::fields(&format!(
                "{kind},Creature-1,Boss,0,0,{player},Player,0,0,123,Fireball,4,999"
            ))
            .expect("fields"),
        }
    }
    #[test]
    fn recaps_are_bounded_player_scoped_and_consumed() {
        let mut recap = DeathRecaps::default();
        for at in [100, 200, 300, 400] {
            recap.ingest(&event(at, "SPELL_DAMAGE", "Player-1"));
        }
        recap.ingest(&event(450, "SPELL_DAMAGE", "Player-2"));
        recap.ingest(&event(460, "SPELL_HEAL", "Player-1"));
        let restored = serde_json::to_string(&recap).expect("checkpoint");
        let mut recap: DeathRecaps = serde_json::from_str(&restored).expect("restore");
        let hits = recap.ingest(&event(500, "UNIT_DIED", "Player-1"));
        assert_eq!(
            hits.iter().map(|hit| hit.offset_ms).collect::<Vec<_>>(),
            [-300, -200, -100]
        );
        assert_eq!(hits[2].ability, "Fireball");
        assert!(
            recap
                .ingest(&event(501, "UNIT_DIED", "Player-1"))
                .is_empty()
        );
        assert_eq!(recap.ingest(&event(502, "UNIT_DIED", "Player-2")).len(), 1);
    }
    #[test]
    fn stale_hits_resurrections_and_zone_changes_do_not_leak() {
        for kind in ["ZONE_CHANGE", "SPELL_RESURRECT"] {
            let mut recap = DeathRecaps::default();
            recap.ingest(&event(0, "SWING_DAMAGE", "Player-1"));
            recap.ingest(&event(1, kind, "Player-1"));
            assert!(recap.ingest(&event(2, "UNIT_DIED", "Player-1")).is_empty());
        }
        let mut recap = DeathRecaps::default();
        recap.ingest(&event(0, "SPELL_DAMAGE", "Player-1"));
        assert!(
            recap
                .ingest(&event(10_001, "UNIT_DIED", "Player-1"))
                .is_empty()
        );
    }
}
