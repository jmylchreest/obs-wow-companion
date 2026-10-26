// Timer snapshot: Raider.IO expansion 11 static-data API, 2026-10-25.
// Capture the limit at run start so updates cannot rewrite historical results.
pub fn limit_ms(id: u32, at: i64) -> Option<i64> {
    match id {
        // Altar of Fangs, The Blinding Vale, Voidscar Arena
        588 | 584 | 585 if (1_787_112_000_000..1_893_456_000_000).contains(&at) => {
            Some(1800 * 1000)
        }
        // Den of Nalorakk, Temple of Sethraliss
        586 | 250 if (1_787_112_000_000..1_893_456_000_000).contains(&at) => Some(1920 * 1000),
        // Kings' Rest
        249 if (1_787_112_000_000..1_893_456_000_000).contains(&at) => Some(1980 * 1000),
        // Murder Row
        587 if (1_787_112_000_000..1_893_456_000_000).contains(&at) => Some(2040 * 1000),
        // Ruby Life Pools
        399 if (1_787_112_000_000..1_893_456_000_000).contains(&at) => Some(1680 * 1000),
        // Algeth'ar Academy
        402 if (1_774_411_200_000..1_787_112_000_000).contains(&at) => Some(1860 * 1000),
        // Magisters' Terrace, Seat of the Triumvirate
        558 | 239 if (1_774_411_200_000..1_787_112_000_000).contains(&at) => Some(2040 * 1000),
        // Maisara Caverns, Windrunner Spire
        560 | 557 if (1_774_411_200_000..1_787_112_000_000).contains(&at) => Some(1980 * 1000),
        // Nexus-Point Xenas, Pit of Saron
        559 | 556 if (1_774_411_200_000..1_787_112_000_000).contains(&at) => Some(1800 * 1000),
        // Skyreach
        161 if (1_774_411_200_000..1_787_112_000_000).contains(&at) => Some(1680 * 1000),
        _ => None,
    }
}
