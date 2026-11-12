//! Alarms, as the calculator keeps them: a date and time, an action and a
//! repeat interval. An appointment alarm (a string, or nothing) shows its
//! message and stays past due until acknowledged; a control alarm runs its
//! object when it comes due.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::calc::{Calc, Error, Res};
use crate::obj::{Obj, r12};

/// The calculator counts time in ticks, 8192 a second.
const TICKS: f64 = 8192.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Alarm {
    /// Seconds since 1970.
    pub at: f64,
    pub action: Obj,
    /// Seconds between repeats, 0 for none.
    pub repeat: f64,
    /// An appointment alarm already announced, waiting for ACK.
    pub due: bool,
}

/// Seconds the calculator's clock runs ahead of the system's, as set by
/// →DATE, →TIME and CLKADJ (the bits of an f64).
static CLOCK: AtomicU64 = AtomicU64::new(0);

pub fn clock_offset() -> f64 {
    f64::from_bits(CLOCK.load(Ordering::Relaxed))
}

pub fn set_clock_offset(s: f64) {
    CLOCK.store(s.to_bits(), Ordering::Relaxed);
}

/// The calculator's clock, in seconds since 1970.
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
        + clock_offset()
}

/// HH.MMSS and MM.DDYYYY as local time, in seconds since 1970.
fn epoch(date: f64, time: f64) -> Res<f64> {
    // Two or four digits at a time, rounded first so that 0.3 is 30, not 29.
    let digits = |x: f64, scale: f64| r12(r12(x).fract() * scale).trunc() as i32;
    let (mon, day, year) = (
        date.trunc() as i32,
        digits(date, 100.0),
        digits(date * 100.0, 1e4),
    );
    let (h, m, s) = (
        time.trunc() as i32,
        digits(time, 100.0),
        digits(time * 100.0, 100.0),
    );
    if !(1..=12).contains(&mon)
        || !(1..=31).contains(&day)
        || !(0..24).contains(&h)
        || m > 59
        || s > 59
    {
        return Err(Error::BadValue);
    }
    // SAFETY: mktime only reads and normalizes the tm it is given.
    let t = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        (tm.tm_year, tm.tm_mon, tm.tm_mday) = (year - 1900, mon - 1, day);
        (tm.tm_hour, tm.tm_min, tm.tm_sec, tm.tm_isdst) = (h, m, s, -1);
        libc::mktime(&mut tm)
    };
    if t == -1 {
        return Err(Error::BadValue);
    }
    Ok(t as f64)
}

/// The day of the week and the date as TSTR writes them: "THU 11/12/26".
fn day_and_date(t: f64) -> (String, String) {
    let secs = t as libc::time_t;
    // SAFETY: localtime_r only writes the tm it is given.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    };
    let day = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"][tm.tm_wday as usize % 7];
    let date = format!(
        "{:02}/{:02}/{:02}",
        tm.tm_mon + 1,
        tm.tm_mday,
        (tm.tm_year + 1900) % 100
    );
    (day.into(), date)
}

fn real_arg(o: &Obj) -> Res<f64> {
    match o {
        Obj::Real(x) => Ok(*x),
        _ => Err(Error::BadType),
    }
}

/// Seconds since 1970 as the date MM.DDYYYY and time HH.MMSS.
pub(crate) fn date_time(t: f64) -> (f64, f64) {
    let secs = t as libc::time_t;
    // SAFETY: localtime_r only writes the tm it is given.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    };
    let date =
        (tm.tm_mon + 1) as f64 + tm.tm_mday as f64 / 100.0 + (tm.tm_year + 1900) as f64 / 1e6;
    let time = tm.tm_hour as f64 + tm.tm_min as f64 / 100.0 + tm.tm_sec as f64 / 1e4;
    (r12(date), r12(time))
}

impl Alarm {
    /// { date time action repeat }, as RCLALARM gives it.
    pub fn to_obj(&self) -> Obj {
        let (d, t) = date_time(self.at);
        Obj::List(vec![
            Obj::Real(d),
            Obj::Real(t),
            self.action.clone(),
            Obj::Real(r12(self.repeat * TICKS)),
        ])
    }

    /// An alarm from a time today, or { [date] time [action [repeat]] }.
    pub fn from_obj(o: &Obj) -> Res<Alarm> {
        let today = date_time(now()).0;
        let (date, time, action, repeat) = match o {
            Obj::Real(t) => (today, *t, Obj::Str(String::new()), 0.0),
            Obj::List(l) => match &l[..] {
                [Obj::Real(t)] => (today, *t, Obj::Str(String::new()), 0.0),
                [Obj::Real(d), Obj::Real(t)] => (*d, *t, Obj::Str(String::new()), 0.0),
                [Obj::Real(d), Obj::Real(t), a] => (*d, *t, a.clone(), 0.0),
                [Obj::Real(d), Obj::Real(t), a, Obj::Real(r)] => (*d, *t, a.clone(), *r),
                _ => return Err(Error::BadValue),
            },
            _ => return Err(Error::BadType),
        };
        Ok(Alarm {
            at: epoch(date, time)?,
            action,
            repeat: repeat.max(0.0) / TICKS,
            due: false,
        })
    }

    pub fn appointment(&self) -> bool {
        matches!(self.action, Obj::Str(_))
    }
}

impl Calc {
    fn alarm_index(&self) -> Res<usize> {
        let Obj::Real(n) = *self.peek(1) else {
            return Err(Error::BadType);
        };
        crate::calc::index(n, self.alarms.len())
    }

    /// The alarms that have come due since last asked: appointments to
    /// announce (they stay until ACK) and control alarms' objects to run
    /// (they go, or come round again if they repeat).
    pub fn due_alarms(&mut self) -> (Vec<String>, Vec<Obj>) {
        let t = now();
        let (mut say, mut run) = (vec![], vec![]);
        for a in &mut self.alarms {
            if a.at > t || a.due {
                continue;
            }
            match &a.action {
                Obj::Str(s) => {
                    say.push(if s.is_empty() {
                        "Alarm".into()
                    } else {
                        s.clone()
                    });
                    a.due = true;
                }
                o => run.push(o.clone()),
            }
            if a.repeat > 0.0 {
                while a.at <= t {
                    a.at += a.repeat;
                }
                a.due = false;
            }
        }
        self.alarms
            .retain(|a| a.repeat > 0.0 || a.appointment() || a.at > t);
        self.alarms.sort_by(|a, b| a.at.total_cmp(&b.at));
        (say, run)
    }

    pub(crate) fn alarm_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            "STOALARM" => self.need(1).and_then(|_| {
                let a = Alarm::from_obj(self.peek(1))?;
                self.stack.pop();
                self.alarms.push(a.clone());
                self.alarms.sort_by(|a, b| a.at.total_cmp(&b.at));
                let n = self.alarms.iter().position(|b| *b == a).unwrap_or(0) + 1;
                self.stack.push(Obj::Real(n as f64));
                Ok(())
            }),
            "RCLALARM" => self.need(1).and_then(|_| {
                let i = self.alarm_index()?;
                self.stack.pop();
                self.stack.push(self.alarms[i].to_obj());
                Ok(())
            }),
            // n DELALARM, or 0 DELALARM for them all.
            "DELALARM" => self.need(1).and_then(|_| {
                if *self.peek(1) == Obj::Real(0.0) {
                    self.alarms.clear();
                } else {
                    let i = self.alarm_index()?;
                    self.alarms.remove(i);
                }
                self.stack.pop();
                Ok(())
            }),
            // The first alarm due at or after a date, { date time }, or 0
            // for the first past due; 0 when there is none.
            "FINDALARM" => self.need(1).and_then(|_| {
                let from = match self.peek(1) {
                    Obj::Real(x) if *x == 0.0 => None,
                    Obj::Real(d) => Some(epoch(*d, 0.0)?),
                    Obj::List(l) => match &l[..] {
                        [Obj::Real(d), Obj::Real(t)] => Some(epoch(*d, *t)?),
                        _ => return Err(Error::BadValue),
                    },
                    _ => return Err(Error::BadType),
                };
                let t = now();
                let n = self
                    .alarms
                    .iter()
                    .position(|a| match from {
                        Some(f) => a.at >= f,
                        None => a.at <= t,
                    })
                    .map_or(0, |i| i + 1);
                self.stack.pop();
                self.stack.push(Obj::Real(n as f64));
                Ok(())
            }),
            // Setting the clock moves the calculator's clock, not the system's.
            "→DATE" | "→TIME" | "CLKADJ" => self.need(1).and_then(|_| {
                let x = real_arg(self.peek(1))?;
                let t = now();
                let (date, time) = date_time(t);
                let target = match w {
                    "→DATE" => epoch(x, time)?,
                    "→TIME" => epoch(date, x)?,
                    _ => t + x / TICKS,
                };
                set_clock_offset(clock_offset() + target - t);
                self.stack.pop();
                Ok(())
            }),
            // date days DATE+: the date that many days later.
            "DATE+" => self.need(2).and_then(|_| {
                let (d, n) = (real_arg(self.peek(2))?, real_arg(self.peek(1))?);
                // From noon, so a change of daylight saving time does not
                // move it across midnight.
                let r = date_time(epoch(d, 12.0)? + n.trunc() * 86400.0).0;
                self.stack.truncate(self.stack.len() - 2);
                self.stack.push(Obj::Real(r));
                Ok(())
            }),
            // date1 date2 DDAYS: the days from date1 to date2.
            "DDAYS" => self.need(2).and_then(|_| {
                let (a, b) = (real_arg(self.peek(2))?, real_arg(self.peek(1))?);
                let days = ((epoch(b, 12.0)? - epoch(a, 12.0)?) / 86400.0).round();
                self.stack.truncate(self.stack.len() - 2);
                self.stack.push(Obj::Real(days));
                Ok(())
            }),
            // date time TSTR: "THU 11/12/26 09:30:00A", or a 24-hour time
            // with flag -41 set.
            "TSTR" => self.need(2).and_then(|_| {
                let (d, t) = (real_arg(self.peek(2))?, real_arg(self.peek(1))?);
                let at = epoch(d, t)?;
                let (day, date) = day_and_date(at);
                let secs = at as i64;
                // SAFETY: localtime_r only writes the tm it is given.
                let tm = unsafe {
                    let mut tm: libc::tm = std::mem::zeroed();
                    libc::localtime_r(&(secs as libc::time_t), &mut tm);
                    tm
                };
                let clock24 = self.flags[1] & (1 << 40) != 0;
                let time = if clock24 {
                    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
                } else {
                    let h = match tm.tm_hour % 12 {
                        0 => 12,
                        h => h,
                    };
                    let ap = if tm.tm_hour < 12 { 'A' } else { 'P' };
                    format!("{h:02}:{:02}:{:02}{ap}", tm.tm_min, tm.tm_sec)
                };
                self.stack.truncate(self.stack.len() - 2);
                self.stack.push(Obj::Str(format!("{day} {date} {time}")));
                Ok(())
            }),
            "ACK" | "ACKALL" => {
                let t = now();
                let mut first = true;
                self.alarms.retain(|a| {
                    let gone = a.at <= t && a.appointment() && (first || w == "ACKALL");
                    first &= !gone;
                    !gone
                });
                Ok(())
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{date_time, epoch, now};
    use crate::calc::Calc;

    #[test]
    fn alarms() {
        let t = epoch(11.112026, 9.3005).unwrap();
        assert_eq!(date_time(t), (11.112026, 9.3005));
        let mut c = Calc::new();
        c.exec_line("{ 1.012000 0 \"Past\" } STOALARM { 12.312999 23 « 1 » } STOALARM")
            .unwrap();
        c.exec_line("1 RCLALARM 0 FINDALARM 12.012999 FINDALARM")
            .unwrap();
        let s: Vec<String> = c.stack.iter().map(|o| o.show(c.fmt)).collect();
        assert_eq!(s, ["1", "2", "{ 1.012 0 \"Past\" 0 }", "1", "2"]);
        let (say, run) = c.due_alarms();
        assert_eq!((say, run.len()), (vec!["Past".to_string()], 0));
        assert!(c.due_alarms().0.is_empty());
        c.exec_line("ACK").unwrap();
        assert_eq!(c.alarms.len(), 1);
        c.exec_line("1 DELALARM").unwrap();
        assert!(c.alarms.is_empty());
        assert!(now() > 1.7e9);
    }

    fn run(src: &str) -> Vec<String> {
        let mut c = Calc::new();
        c.exec_line(src).unwrap();
        c.stack.iter().map(|o| o.show(c.fmt)).collect()
    }

    #[test]
    fn dates() {
        assert_eq!(
            run("12.312026 1 DATE+ 2.282028 1 DATE+"),
            ["1.012027", "2.292028"]
        );
        assert_eq!(run("1.012026 11.122026 DDAYS"), ["315"]);
        assert_eq!(
            run("11.122026 21.3005 TSTR"),
            ["\"THU 11/12/26 09:30:05P\""]
        );
        assert_eq!(
            run("-41 SF 11.122026 21.3005 TSTR"),
            ["\"THU 11/12/26 21:30:05\""]
        );
    }

    #[test]
    fn setting_the_clock() {
        let mut c = Calc::new();
        c.exec_line("2.032031 →DATE DATE").unwrap();
        let date = c.stack[0].show(c.fmt);
        super::set_clock_offset(0.0);
        assert_eq!(date, "2.032031");
    }
}
