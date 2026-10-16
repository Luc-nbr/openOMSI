//! A tender's auction (Luc: when the player bids on a line, fictitious companies bid too; he
//! may buy the line directly, or bid with the chance to lose it; and it lasts a few hours, not
//! days).
//!
//! A line is auctioned for a few hours of the company's time. The bidders ask a price a year for
//! running the line (at most the authority's published cap, the `ceiling`: a bid above it does
//! not count), and the authority scores every bid 70 % on price and 30 % on quality (Luc
//! 2026-10-15, and a county council's terms): the lowest ask gets the full 70 price points, the
//! others `lowest / ask` of them; the quality points are the bidder's performance - 100 % less
//! 2 % for each average net default point of its contracts over the last twelve months
//! (`bus_contracts::quality`) - of 30. The best total when the auction closes wins; on equal
//! totals the better price points, then the better quality (`points`). Each new bid has to lead
//! by `MARGIN` points, so the last bid placed always leads.
//!
//! The rivals are the map's other operators, each with a character: an aggressive one answers
//! fast and goes low, a cautious one takes its time and stays high, a big one can afford a thin
//! margin, a small one not; some keep quiet until the last minutes and only then bid. Each knows
//! how low it will go (its floor: the work's cost price on the UK operator's formula, within the
//! game mode's band around it by its character - `Character::band`; drawn per tender and kept
//! secret); the player sees their characters and their bids as they come, and what `chance`
//! reckons from what he can know.
//!
//! Everything is replayed from the auction's start (`replay`): the same bids of the player
//! give the same auction, and a bid of his changes only what comes after it (the rivals act
//! before him within a minute). Minutes are the company clock's (`clock::now`).

use super::model::Cents;
use super::rng::Rng;
use serde::{Deserialize, Serialize};

/// The points of the price and of the quality (of 100).
pub const PRICE_POINTS: f64 = 70.0;
pub const QUALITY_POINTS: f64 = 30.0;
/// A new bid leads the best one by this many points at least.
pub const MARGIN: f64 = 0.4;

/// How an operator bids.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Character {
    Aggressive,
    #[default]
    Cautious,
    Big,
    Small,
}

impl Character {
    pub const ALL: [Character; 4] = [Character::Aggressive, Character::Cautious, Character::Big, Character::Small];

    pub fn label(self) -> &'static str {
        match self {
            Character::Aggressive => "aggressive",
            Character::Cautious => "cautious",
            Character::Big => "big",
            Character::Small => "small",
        }
    }

    /// Where its floor lies in the game mode's band of bids around the cost price
    /// (`econ_uk::ModeFactors::rival_bids`): from, to, as fractions of the band (0 its lowest,
    /// 1 its highest).
    pub fn band(self) -> (f64, f64) {
        match self {
            Character::Aggressive => (0.0, 0.5),
            Character::Big => (0.15, 0.65),
            Character::Small => (0.35, 0.85),
            Character::Cautious => (0.5, 1.0),
        }
    }

    /// Minutes it takes to answer a bid that beat it (from, to).
    pub fn delay(self) -> (i64, i64) {
        match self {
            Character::Aggressive => (3, 15),
            Character::Big => (10, 35),
            Character::Cautious => (25, 80),
            Character::Small => (15, 50),
        }
    }

    /// How likely it keeps quiet until the last minutes.
    pub fn quiet(self) -> f64 {
        match self {
            Character::Aggressive => 0.35,
            Character::Cautious => 0.4,
            Character::Big => 0.1,
            Character::Small => 0.2,
        }
    }

    /// Steps below the most it may ask it goes, at most.
    pub fn jump(self) -> i64 {
        match self {
            Character::Aggressive => 2,
            Character::Big => 1,
            _ => 0,
        }
    }

    /// Its quality in the authority's eyes (from, to): its performance, 100 % less 2 % for each
    /// average net default point (an aggressive one cuts corners, a cautious one runs clean).
    pub fn quality(self) -> (f64, f64) {
        match self {
            Character::Aggressive => (76.0, 94.0),
            Character::Big => (84.0, 98.0),
            Character::Cautious => (88.0, 100.0),
            Character::Small => (78.0, 96.0),
        }
    }
}

/// The price points of an ask, the lowest ask being `lowest`.
pub fn price_points(amount: Cents, lowest: Cents) -> f64 {
    if amount <= 0 {
        return PRICE_POINTS;
    }
    PRICE_POINTS * (lowest.max(1) as f64 / amount as f64).min(1.0)
}

/// The quality points of a bidder of that quality (0-100).
pub fn quality_points(quality: f64) -> f64 {
    QUALITY_POINTS * (quality / 100.0).clamp(0.0, 1.0)
}

/// A bid's total.
pub fn score(amount: Cents, quality: f64, lowest: Cents) -> f64 {
    price_points(amount, lowest) + quality_points(quality)
}

/// The auction's frame: when it runs (minutes), the most the authority pays (its published cap,
/// a year) and the step a rival may go lower by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lot {
    pub opens: i64,
    /// Bids are taken until the minute before.
    pub closes: i64,
    pub ceiling: Cents,
    pub step: Cents,
}

/// A rival as the replay knows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Bidder {
    pub character: Character,
    /// The least it asks (secret).
    pub floor: Cents,
    pub quality: f64,
    /// Its own draws (its answers' delays and jumps).
    pub seed: u64,
    /// The minute of its opening bid, if it opens.
    pub opening: Option<i64>,
    /// The minute it bids at if it keeps quiet until then.
    pub quiet_until: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Rival(usize),
    Player,
}

/// A bid placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub at: i64,
    pub who: Who,
    pub amount: Cents,
}

/// Bids go in whole hundreds of euros.
pub const ROUND: Cents = 100_00;

/// The most a bidder of `quality` may ask to lead the standing bids `others` (ask, quality) by
/// `MARGIN` points (none standing: the ceiling), in whole hundreds.
pub fn to_lead(others: &[(Cents, f64)], quality: f64, lot: &Lot) -> Cents {
    let top = (lot.ceiling / ROUND).max(1);
    if others.is_empty() {
        return top * ROUND;
    }
    let low_other = others.iter().map(|o| o.0).min().unwrap_or(Cents::MAX);
    let leads = |k: i64| {
        let x = k * ROUND;
        let lowest = low_other.min(x);
        let best = others.iter().map(|o| score(o.0, o.1, lowest)).fold(f64::MIN, f64::max);
        score(x, quality, lowest) >= best + MARGIN
    };
    // (the lower, the better its lead: the most that still leads)
    let (mut lo, mut hi) = (1, top);
    if !leads(lo) {
        return ROUND;
    }
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if leads(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo * ROUND
}

/// The standing bids (the last of each bidder) but `who`'s: ask and quality.
fn standing(last: &[Option<Cents>], rivals: &[Bidder], player: Option<Cents>, player_quality: f64, who: Who) -> Vec<(Cents, f64)> {
    let mut out: Vec<(Cents, f64)> = last.iter().enumerate().filter(|(i, _)| who != Who::Rival(*i)).filter_map(|(i, a)| a.map(|a| (a, rivals[i].quality))).collect();
    if who != Who::Player {
        out.extend(player.map(|a| (a, player_quality)));
    }
    out
}

/// The auction from its start until the minute `until` (inclusive; at most its last minute):
/// the rivals' bids, and the player's (`player`: when, how much; a bid that does not lead
/// when it comes is left out).
pub fn replay(lot: &Lot, rivals: &[Bidder], player: &[(i64, Cents)], player_quality: f64, until: i64) -> Vec<Placed> {
    let end = until.min(lot.closes - 1);
    let n = rivals.len();
    let mut rngs: Vec<Rng> = rivals.iter().map(|b| Rng::new(b.seed)).collect();
    let mut next: Vec<Option<i64>> = rivals.iter().map(|b| if b.quiet_until.is_some() { None } else { b.opening }).collect();
    let mut out = vec![false; n];
    let mut woke = vec![false; n];
    let mut placed: Vec<Placed> = Vec::new();
    let mut last: Vec<Option<Cents>> = vec![None; n];
    let mut mine: Option<Cents> = None;
    let mut leader: Option<Who> = None;
    let mut m = lot.opens;
    while m <= end {
        for i in 0..n {
            let wakes = rivals[i].quiet_until == Some(m) && !woke[i];
            if next[i] != Some(m) && !wakes {
                continue;
            }
            if wakes {
                woke[i] = true;
            }
            next[i] = None;
            if out[i] || leader == Some(Who::Rival(i)) {
                continue;
            }
            let need = to_lead(&standing(&last, rivals, mine, player_quality, Who::Rival(i)), rivals[i].quality, lot);
            if need < rivals[i].floor {
                out[i] = true;
                continue;
            }
            let extra = rngs[i].int(0, rivals[i].character.jump()) * lot.step;
            let amount = (need - extra).max(rivals[i].floor);
            placed.push(Placed { at: m, who: Who::Rival(i), amount });
            last[i] = Some(amount);
            leader = Some(Who::Rival(i));
            answer(&mut next, &mut rngs, rivals, &out, &woke, Some(i), m);
        }
        for &(_, amount) in player.iter().filter(|p| p.0 == m) {
            let need = to_lead(&standing(&last, rivals, mine, player_quality, Who::Player), player_quality, lot);
            if amount >= ROUND && amount <= need {
                placed.push(Placed { at: m, who: Who::Player, amount });
                mine = Some(amount);
                leader = Some(Who::Player);
                answer(&mut next, &mut rngs, rivals, &out, &woke, None, m);
            }
        }
        // (on to the next minute anything happens)
        let soonest = next.iter().flatten().copied().chain(rivals.iter().zip(&woke).filter(|(_, w)| !**w).filter_map(|(b, _)| b.quiet_until)).chain(player.iter().map(|p| p.0)).filter(|t| *t > m).min();
        m = match soonest {
            Some(t) => t,
            None => break,
        };
    }
    placed
}

/// The rivals beaten (not `by`) answer after their delay; a quiet one keeps waiting.
fn answer(next: &mut [Option<i64>], rngs: &mut [Rng], rivals: &[Bidder], out: &[bool], woke: &[bool], by: Option<usize>, m: i64) {
    for j in 0..rivals.len() {
        if Some(j) == by || out[j] || (rivals[j].quiet_until.is_some() && !woke[j]) {
            continue;
        }
        let (a, b) = rivals[j].character.delay();
        let t = m + rngs[j].int(a, b);
        if next[j].is_none_or(|x| x > t) {
            next[j] = Some(t);
        }
    }
}

/// The last bid of each bidder in `placed` (the rivals by index, then the player's).
pub fn last_of(placed: &[Placed], n: usize) -> (Vec<Option<Cents>>, Option<Cents>) {
    let mut last = vec![None; n];
    let mut mine = None;
    for p in placed {
        match p.who {
            Who::Rival(i) if i < n => last[i] = Some(p.amount),
            Who::Player => mine = Some(p.amount),
            _ => {}
        }
    }
    (last, mine)
}

/// The most the player may ask now to lead.
pub fn player_to_lead(placed: &[Placed], rivals: &[Bidder], player_quality: f64, lot: &Lot) -> Cents {
    let (last, mine) = last_of(placed, rivals.len());
    to_lead(&standing(&last, rivals, mine, player_quality, Who::Player), player_quality, lot)
}

/// A bidder's standing: its last ask and its points (price, quality, total).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Points {
    pub who: Who,
    pub amount: Cents,
    pub price: f64,
    pub quality: f64,
    pub total: f64,
}

/// Every bidder's points from its last bid, best first.
pub fn points(placed: &[Placed], rivals: &[Bidder], player_quality: f64) -> Vec<Points> {
    let (last, mine) = last_of(placed, rivals.len());
    let mut all: Vec<(Who, Cents, f64)> = last.iter().enumerate().filter_map(|(i, a)| a.map(|a| (Who::Rival(i), a, rivals[i].quality))).collect();
    all.extend(mine.map(|a| (Who::Player, a, player_quality)));
    let Some(lowest) = all.iter().map(|x| x.1).min() else { return Vec::new() };
    let mut out: Vec<Points> = all.into_iter().map(|(who, amount, q)| Points { who, amount, price: price_points(amount, lowest), quality: quality_points(q), total: score(amount, q, lowest) }).collect();
    // (equal totals: the better price points, then the better quality)
    out.sort_by(|a, b| b.total.total_cmp(&a.total).then(b.price.total_cmp(&a.price)).then(b.quality.total_cmp(&a.quality)));
    out
}

/// What a rival looks like to the player: where its floor may lie (the cost price's band for its
/// character, cents), its quality and its last bid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub floor: (f64, f64),
    pub quality: f64,
    pub last: Option<Cents>,
}

/// The player's chance to win with an ask of `amount` now, as he can reckon it: every rival's
/// floor lies somewhere in its range (`Seen::floor`), at most what it bid already; it beats him if
/// it can ask low enough to lead his bid (a floor above the cap never bids).
pub fn chance(amount: Cents, player_quality: f64, rivals: &[Seen], lot: &Lot) -> f64 {
    let mut p = 1.0;
    for r in rivals {
        let (lo, hi) = r.floor;
        let hi = hi.min(r.last.map_or(f64::MAX, |l| l as f64));
        let need = to_lead(&[(amount, player_quality)], r.quality, lot) as f64;
        // (it beats him when its floor is at most what it needs)
        let above = if hi <= lo { if lo > need { 1.0 } else { 0.0 } } else { ((hi - need) / (hi - lo)).clamp(0.0, 1.0) };
        p *= above;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lot() -> Lot {
        Lot { opens: 600, closes: 600 + 240, ceiling: 100_000_00, step: 1_000_00 }
    }

    fn rival(ch: Character, floor: Cents, seed: u64, opening: Option<i64>, quiet: Option<i64>) -> Bidder {
        Bidder { character: ch, floor, quality: 60.0, seed, opening, quiet_until: quiet }
    }

    #[test]
    fn the_score_is_seventy_price_and_thirty_quality() {
        // the lowest ask gets the 70 price points, another its share of them
        assert_eq!(price_points(80_000_00, 80_000_00), 70.0);
        assert!((price_points(100_000_00, 80_000_00) - 56.0).abs() < 1e-9);
        assert_eq!(quality_points(100.0), 30.0);
        // a cheaper bid of lower quality still wins: 5 % cheaper against 10 quality points
        let (cheap, good) = ((95_000_00, 50.0), (100_000_00, 60.0));
        let low = cheap.0.min(good.0);
        assert!(score(cheap.0, cheap.1, low) > score(good.0, good.1, low));
        // ... but a much worse quality loses against a slightly dearer bid
        let (cheap, good) = ((97_000_00, 20.0), (100_000_00, 80.0));
        let low = cheap.0.min(good.0);
        assert!(score(cheap.0, cheap.1, low) < score(good.0, good.1, low));
        // to lead a bid of better quality the player asks less than it
        let l = lot();
        let need = to_lead(&[(90_000_00, 80.0)], 50.0, &l);
        assert!(need < 90_000_00 && need > 70_000_00, "{need}");
        let lowest = need.min(90_000_00);
        assert!(score(need, 50.0, lowest) >= score(90_000_00, 80.0, lowest) + MARGIN);
        let more = need + ROUND;
        assert!(score(more, 50.0, more.min(90_000_00)) < score(90_000_00, 80.0, more.min(90_000_00)) + MARGIN);
        assert_eq!(to_lead(&[], 50.0, &l), l.ceiling);
    }

    #[test]
    fn rivals_answer_down_to_their_floor_and_the_last_bid_leads() {
        let l = Lot { closes: 600 + 900, ..lot() };
        let rivals = vec![rival(Character::Aggressive, 80_000_00, 1, Some(610), None), rival(Character::Cautious, 90_000_00, 2, None, None)];
        let p = replay(&l, &rivals, &[], 60.0, l.closes);
        assert_eq!((p[0].at, p[0].who), (610, Who::Rival(0)));
        assert!(p[0].amount <= l.ceiling && p[0].amount >= l.ceiling - 2 * l.step);
        assert!(p.len() >= 3);
        assert!(p.windows(2).all(|w| w[0].at <= w[1].at && w[1].amount < w[0].amount), "{p:?}");
        assert!(p.iter().all(|b| b.at < l.closes));
        assert!(p.iter().all(|b| b.amount >= rivals[match b.who { Who::Rival(i) => i, _ => unreachable!() }].floor));
        assert_eq!(p.last().unwrap().who, Who::Rival(0), "the one going lower leads at the end");
        assert_eq!(p, replay(&l, &rivals, &[], 60.0, l.closes));
        let half = replay(&l, &rivals, &[], 60.0, 700);
        assert_eq!(&p[..half.len()], &half[..]);
        // the points: the winner first, with the full price points
        let pts = points(&p, &rivals, 60.0);
        assert_eq!(pts[0].who, Who::Rival(0));
        assert_eq!(pts[0].price, PRICE_POINTS);
    }

    #[test]
    fn a_bid_of_the_player_changes_only_what_comes_after() {
        let l = lot();
        let rivals = vec![rival(Character::Big, 85_000_00, 7, Some(620), None), rival(Character::Small, 92_000_00, 8, Some(700), None)];
        let before = replay(&l, &rivals, &[], 60.0, l.closes);
        let at = 650;
        let mine = replay(&l, &rivals, &[(at, 60_000_00)], 60.0, l.closes);
        let n = before.iter().filter(|b| b.at <= at).count();
        assert_eq!(&before[..n], &mine[..n]);
        // a bid below every floor wins
        assert_eq!(mine.last().unwrap().who, Who::Player);
        // a bid that does not lead is not taken
        let high = replay(&l, &rivals, &[(at, l.ceiling + ROUND)], 60.0, l.closes);
        assert!(!high.iter().any(|b| b.who == Who::Player));
    }

    #[test]
    fn a_quiet_rival_bids_in_the_last_minutes() {
        let l = lot();
        let rivals = vec![rival(Character::Aggressive, 70_000_00, 3, None, Some(l.closes - 5))];
        let p = replay(&l, &rivals, &[(620, 95_000_00)], 60.0, l.closes);
        assert_eq!(p.len(), 2);
        assert_eq!((p[1].at, p[1].who), (l.closes - 5, Who::Rival(0)));
        // a better name counts: it may ask more and still lead
        assert!(to_lead(&[(90_000_00, 40.0)], 90.0, &l) > to_lead(&[(90_000_00, 90.0)], 40.0, &l));
    }

    #[test]
    fn the_chance_is_what_comes_out() {
        let l = Lot { opens: 0, closes: 300, ceiling: 100_000_00, step: 1_000_00 };
        let chars = [Character::Aggressive, Character::Cautious, Character::Small];
        // (floors within -15 % and +15 % of a cost price of 85,000, by character)
        let range = |c: &Character| {
            let (a, b) = c.band();
            (85_000_00.0 * (0.85 + 0.3 * a), 85_000_00.0 * (0.85 + 0.3 * b))
        };
        for amount in [95_000_00, 85_000_00, 75_000_00] {
            let seen: Vec<Seen> = chars.iter().map(|c| Seen { floor: range(c), quality: 55.0, last: None }).collect();
            let reckoned = chance(amount, 55.0, &seen, &l);
            let mut wins = 0;
            let n = 600;
            for k in 0..n {
                let mut rng = Rng::new(k as u64 * 7919 + amount as u64);
                let rivals: Vec<Bidder> = chars
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let (lo, hi) = range(c);
                        Bidder { character: *c, floor: rng.range(lo, hi) as Cents, quality: 55.0, seed: rng.next_u64() ^ i as u64, opening: None, quiet_until: None }
                    })
                    .collect();
                let p = replay(&l, &rivals, &[(1, amount)], 55.0, l.closes);
                if p.last().is_some_and(|b| b.who == Who::Player) {
                    wins += 1;
                }
            }
            let got = wins as f64 / n as f64;
            assert!((got - reckoned).abs() < 0.08, "{amount}: reckoned {reckoned:.2}, won {got:.2}");
        }
        // less is a better chance; below every floor it is certain
        let seen = [Seen { floor: range(&Character::Big), quality: 55.0, last: Some(88_000_00) }];
        assert!(chance(86_000_00, 55.0, &seen, &l) < chance(80_000_00, 55.0, &seen, &l));
        assert_eq!(chance(60_000_00, 55.0, &seen, &l), 1.0);
        assert_eq!(chance(95_000_00, 55.0, &seen, &l), 0.0, "above what it bid already");
    }
}
