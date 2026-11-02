//! `TZ` selects the Local zone, and named zones load from the system's time zone database. Expected
//! output is the Go build's. Skipped where the system has no database, as Go would then fall back.

use std::path::Path;
use std::process::Command;

const SCRIPT: &str = r#"fmt := import("fmt")
times := import("times")
show := func(t) { fmt.println(times.time_string(t), " | ", times.time_location(t), " ", times.time_unix(t)) }
show(times.date(1982, 9, 28, 19, 21, 44, 999))
show(times.date(2024, 1, 15, 12, 0, 0, 0))
show(times.date(2024, 7, 15, 12, 0, 0, 0))
show(times.date(2024, 3, 10, 2, 30, 0, 0))
show(times.date(2100, 7, 1, 0, 0, 0, 0))
show(times.to_local(times.date(2024, 7, 15, 12, 0, 0, 0, "UTC")))
show(times.date(1982, 9, 28, 19, 21, 44, 999, "Pacific/Auckland"))
show(times.in_location(times.date(1982, 9, 28, 19, 21, 44, 999, "UTC"), "Pacific/Auckland"))
show(times.parse(times.format_rfc1123, "Mon, 02 Jul 2006 15:04:05 EDT"))
show(times.parse(times.format_rfc1123z, "Mon, 02 Jan 2006 15:04:05 +0900"))
"#;

#[track_caller]
fn check(tz: &str, expected: &str) {
    if !Path::new("/usr/share/zoneinfo/Asia/Tokyo").exists() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("tengo-tz-test-{}-{}", std::process::id(), tz.replace('/', "-")));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tengo"), SCRIPT).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tengo")).arg("main.tengo").current_dir(&dir).env("TZ", tz).output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "TZ={}", tz);
}

#[test]
fn utc() {
    check("UTC", r#"1982-09-28 19:21:44.000000999 +0000 UTC | UTC 402088904
2024-01-15 12:00:00 +0000 UTC | UTC 1705320000
2024-07-15 12:00:00 +0000 UTC | UTC 1721044800
2024-03-10 02:30:00 +0000 UTC | UTC 1710037800
2100-07-01 00:00:00 +0000 UTC | UTC 4118083200
2024-07-15 12:00:00 +0000 UTC | UTC 1721044800
1982-09-28 19:21:44.000000999 +1200 NZST | Pacific/Auckland 402045704
1982-09-29 07:21:44.000000999 +1200 NZST | Pacific/Auckland 402088904
2006-07-02 15:04:05 +0000 EDT | EDT 1151852645
2006-01-02 15:04:05 +0900 +0900 |  1136181845
"#);
}

#[test]
fn empty_is_utc() {
    check("", r#"1982-09-28 19:21:44.000000999 +0000 UTC | UTC 402088904
2024-01-15 12:00:00 +0000 UTC | UTC 1705320000
2024-07-15 12:00:00 +0000 UTC | UTC 1721044800
2024-03-10 02:30:00 +0000 UTC | UTC 1710037800
2100-07-01 00:00:00 +0000 UTC | UTC 4118083200
2024-07-15 12:00:00 +0000 UTC | UTC 1721044800
1982-09-28 19:21:44.000000999 +1200 NZST | Pacific/Auckland 402045704
1982-09-29 07:21:44.000000999 +1200 NZST | Pacific/Auckland 402088904
2006-07-02 15:04:05 +0000 EDT | EDT 1151852645
2006-01-02 15:04:05 +0900 +0900 |  1136181845
"#);
}

#[test]
fn unknown_falls_back_to_utc() {
    check("Nope/Zone", r#"1982-09-28 19:21:44.000000999 +0000 UTC | UTC 402088904
2024-01-15 12:00:00 +0000 UTC | UTC 1705320000
2024-07-15 12:00:00 +0000 UTC | UTC 1721044800
2024-03-10 02:30:00 +0000 UTC | UTC 1710037800
2100-07-01 00:00:00 +0000 UTC | UTC 4118083200
2024-07-15 12:00:00 +0000 UTC | UTC 1721044800
1982-09-28 19:21:44.000000999 +1200 NZST | Pacific/Auckland 402045704
1982-09-29 07:21:44.000000999 +1200 NZST | Pacific/Auckland 402088904
2006-07-02 15:04:05 +0000 EDT | EDT 1151852645
2006-01-02 15:04:05 +0900 +0900 |  1136181845
"#);
}

#[test]
fn tokyo() {
    check("Asia/Tokyo", r#"1982-09-28 19:21:44.000000999 +0900 JST | Asia/Tokyo 402056504
2024-01-15 12:00:00 +0900 JST | Asia/Tokyo 1705287600
2024-07-15 12:00:00 +0900 JST | Asia/Tokyo 1721012400
2024-03-10 02:30:00 +0900 JST | Asia/Tokyo 1710005400
2100-07-01 00:00:00 +0900 JST | Asia/Tokyo 4118050800
2024-07-15 21:00:00 +0900 JST | Asia/Tokyo 1721044800
1982-09-28 19:21:44.000000999 +1200 NZST | Pacific/Auckland 402045704
1982-09-29 07:21:44.000000999 +1200 NZST | Pacific/Auckland 402088904
2006-07-02 15:04:05 +0000 EDT | EDT 1151852645
2006-01-02 15:04:05 +0900 JST | Asia/Tokyo 1136181845
"#);
}

#[test]
fn new_york_with_dst() {
    check("America/New_York", r#"1982-09-28 19:21:44.000000999 -0400 EDT | America/New_York 402103304
2024-01-15 12:00:00 -0500 EST | America/New_York 1705338000
2024-07-15 12:00:00 -0400 EDT | America/New_York 1721059200
2024-03-10 01:30:00 -0500 EST | America/New_York 1710052200
2100-07-01 00:00:00 -0400 EDT | America/New_York 4118097600
2024-07-15 08:00:00 -0400 EDT | America/New_York 1721044800
1982-09-28 19:21:44.000000999 +1200 NZST | Pacific/Auckland 402045704
1982-09-29 07:21:44.000000999 +1200 NZST | Pacific/Auckland 402088904
2006-07-02 15:04:05 -0400 EDT | America/New_York 1151867045
2006-01-02 15:04:05 +0900 +0900 |  1136181845
"#);
}

#[test]
fn sydney_southern_dst() {
    check("Australia/Sydney", r#"1982-09-28 19:21:44.000000999 +1000 AEST | Australia/Sydney 402052904
2024-01-15 12:00:00 +1100 AEDT | Australia/Sydney 1705280400
2024-07-15 12:00:00 +1000 AEST | Australia/Sydney 1721008800
2024-03-10 02:30:00 +1100 AEDT | Australia/Sydney 1709998200
2100-07-01 00:00:00 +1000 AEST | Australia/Sydney 4118047200
2024-07-15 22:00:00 +1000 AEST | Australia/Sydney 1721044800
1982-09-28 19:21:44.000000999 +1200 NZST | Pacific/Auckland 402045704
1982-09-29 07:21:44.000000999 +1200 NZST | Pacific/Auckland 402088904
2006-07-02 15:04:05 +0000 EDT | EDT 1151852645
2006-01-02 15:04:05 +0900 +0900 |  1136181845
"#);
}

#[test]
fn kolkata_half_hour() {
    check("Asia/Kolkata", r#"1982-09-28 19:21:44.000000999 +0530 IST | Asia/Kolkata 402069104
2024-01-15 12:00:00 +0530 IST | Asia/Kolkata 1705300200
2024-07-15 12:00:00 +0530 IST | Asia/Kolkata 1721025000
2024-03-10 02:30:00 +0530 IST | Asia/Kolkata 1710018000
2100-07-01 00:00:00 +0530 IST | Asia/Kolkata 4118063400
2024-07-15 17:30:00 +0530 IST | Asia/Kolkata 1721044800
1982-09-28 19:21:44.000000999 +1200 NZST | Pacific/Auckland 402045704
1982-09-29 07:21:44.000000999 +1200 NZST | Pacific/Auckland 402088904
2006-07-02 15:04:05 +0000 EDT | EDT 1151852645
2006-01-02 15:04:05 +0900 +0900 |  1136181845
"#);
}
