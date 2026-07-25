use server::config::Thresholds;
use server::stats::ranks::{Side, active, band_for, holders};

#[test]
fn probe() {
    let t = Thresholds::default();
    // bands are narrowest-first and nested: band i's exclusive places are (holders(i-1), holders(i)]
    for e in 1..5000u64 {
        let h: Vec<u64> = t.bands.iter().map(|b| holders(b, e)).collect();
        for (i, band) in t.bands.iter().enumerate() {
            // no band may hold more than share*eligible at either end, and never round up
            assert!(
                (h[i] as f64) <= band.share * e as f64,
                "over-award {} at {e}: {} > {}",
                band.high, h[i], band.share * e as f64
            );
            assert!(h[i] >= 1 || active(e, &t).iter().all(|s| *s != band.high.as_str()),
                "{} listed active at {e} with zero holders", band.high);
            let inner = if i == 0 { 0 } else { h[i - 1] };
            let hi = (1..=e).filter(|p| band_for(*p, e, &t).map(|a| (a.slug(), a.side)) == Some((band.high.as_str(), Side::High))).count() as u64;
            let lo = (1..=e).filter(|p| band_for(*p, e, &t).map(|a| (a.slug(), a.side)) == Some((band.low.as_str(), Side::Low))).count() as u64;
            assert_eq!(hi, h[i] - inner, "high {} at {e}", band.high);
            assert_eq!(lo, h[i] - inner, "low {} at {e}", band.low);
        }
        // total titled at each end equals the widest band's holder count -> no place double-counted
        let titled = (1..=e).filter(|p| band_for(*p, e, &t).is_some()).count() as u64;
        assert_eq!(titled, 2 * h[t.bands.len() - 1], "titled at {e}");
    }
    assert_eq!(holders(&t.bands[0], 720), 0);
    assert_eq!(band_for(1, 720, &t).map(|a| a.slug()), Some("loosh"));
    assert_eq!(band_for(1, 999, &t).map(|a| a.slug()), Some("loosh"));
    assert_eq!(band_for(1, 1000, &t).map(|a| a.slug()), Some("annunaki"));
    assert_eq!(band_for(1000, 1000, &t).map(|a| a.slug()), Some("kartoffel"));
    println!("all band invariants hold for 1..5000 eligible");
}
