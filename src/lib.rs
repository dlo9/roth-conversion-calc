#![feature(test)]
extern crate test;

use pathfinding::prelude::astar;
use chrono::Duration;
use chrono::Datelike;
use chrono::naive::NaiveDate;
use failure::*;

pub struct ProjectArgs {
    // TODO: make Vec
    yearly_taxable_income_excluding_ira: u64,
    inflation_effective_annual_rate: f64,
    roth_present_value: u64,
    roth_effective_annual_rate: f64,
    ira_present_value: u64,
    ira_effective_annual_rate: f64,
    birthday: NaiveDate,
    // TODO: these dates should ALWAYS be Dec 31. 
    end_date: NaiveDate,
    now: NaiveDate,
}

impl ProjectArgs {
    #[allow(unused_comparisons)]
    fn validate(&self) -> Result<(), Error> {
        // TODO: make custom types with validation ranges (from macro?), checked operations
        Err(if self.yearly_taxable_income_excluding_ira < 0 {
            err_msg("Yearly taxable income must be >= 0")
        } else if self.inflation_effective_annual_rate > 1.0 || self.inflation_effective_annual_rate < 0.0 {
            err_msg("Inflation must be between 0 and 1")
        } else if self.roth_present_value < 0{
            err_msg("Roth value must be >= 0")
        } else if self.roth_effective_annual_rate > 1.0 || self.roth_effective_annual_rate < 0.0 {
            err_msg("Roth rate must be between 0 and 1")
        } else if self.ira_present_value < 0{
            err_msg("IRA value must be >= 0")
        } else if self.ira_effective_annual_rate > 1.0 || self.ira_effective_annual_rate < 0.0 {
            err_msg("IRA rate must be between 0 and 1")
        } else if self.birthday > self.now {
            err_msg("Birthday must be <= now")
        } else if self.end_date < self.now {
            err_msg("End date must be >= now")
        } else {
            return Ok(())
        })
    }
}

// TODO: ira needs a basis amount
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct State {
    adjusted_spendable_income: u64,
    pending_rollover: u64,
    now: NaiveDate,
    roth_present_value: u64,
    ira_present_value: u64,
}

type Cost = u64;

impl State {
    fn step_time(&self, args: &ProjectArgs) -> Result<(State, Cost), Error> {
        let now = self.now.checked_add_signed(Duration::days(365)).ok_or_else(|| err_msg("overflow"))?;
        // TODO: is the rollover & RMD meshing properly?
        let ira_rmd = get_rmd(args.birthday, now, self.ira_present_value).checked_sub(self.pending_rollover).unwrap_or_default();
        let ira_value = ((self.ira_present_value as f64) * (1f64 + args.ira_effective_annual_rate - args.inflation_effective_annual_rate)) as u64;
        let ira_value = ira_value - self.pending_rollover - ira_rmd;

        let roth_value = ((self.roth_present_value as f64) * (1f64 + args.roth_effective_annual_rate - args.inflation_effective_annual_rate)) as u64;
        let roth_value = roth_value + self.pending_rollover;

        let taxable_income = args.yearly_taxable_income_excluding_ira + self.pending_rollover + ira_rmd;
        let tax = get_tax(taxable_income);

        Ok((State {
            adjusted_spendable_income: self.adjusted_spendable_income + taxable_income,
            pending_rollover: 0,
            roth_present_value: roth_value,
            ira_present_value: ira_value,
            now: now,
            // TODO: want to maximize income, not minimize tax
        }, tax)) 
    }

    fn step_rollover(&self, rollover_amount: u64) -> Option<(State, Cost)> {
        let pending_rollover = rollover_amount + self.pending_rollover;
        if self.ira_present_value > pending_rollover {
            Some((State {
                pending_rollover: pending_rollover,
                .. *self
            }, 0))
        } else {
            None
        }

    }

    fn successors(&self, args: &ProjectArgs) -> Result<Vec<(State, Cost)>, Error> {
        // TODO: place in args
        let rollover_amount = 1000;

        Ok(if self.now >= args.end_date {
            vec![]
        } else {
            vec![
                self.step_time(args).ok(),
                self.step_rollover(rollover_amount),
            ].into_iter().filter_map(|x| x).collect()
        })
    }
}

// TODO: #[wasm_bindgen]
pub fn project(args: ProjectArgs) -> Option<(Vec<State>, Cost)> {
    if args.validate().is_err() {
        return None;
    }

    let start = State {
        adjusted_spendable_income: 0,
        pending_rollover: 0,
        // TODO: Pass in from args instead, so tests are reproducible
        now: args.now,
        roth_present_value: args.roth_present_value,
        ira_present_value: args.ira_present_value,
    };

    dbg!(astar(&start,
               |ref s| s.successors(&args).unwrap(),
               // TODO: improve
               |ref s| get_tax(args.yearly_taxable_income_excluding_ira + s.pending_rollover),
               |ref s| s.now >= args.end_date,
               ))
}

// TODO: only applies if (spouse not sole beneficiary) || (their age >= your age - 10)
// Worksheet: https://www.irs.gov/pub/irs-tege/uniform_rmd_wksht.pdf
fn get_rmd_distribution_period(birthday: NaiveDate, current_year: i32) -> Option<f64> {
    // TODO: lazy_static
    // Index 0 == age 70
    let distribution_periods = [
        27.4, 26.5, 25.6, 24.7, 23.8, 22.9, 22.0, 21.2, 20.3, 19.5, 18.7, 17.9,
        17.1, 16.3, 15.5, 14.8, 14.1, 13.4, 12.7, 12.0, 11.4, 10.8, 10.2, 9.6,
        9.1, 8.6, 8.1, 7.6, 7.1, 6.7, 6.3, 5.9, 5.5, 5.2, 4.9, 4.5,
        4.2, 3.9, 3.7, 3.4, 3.1, 2.9, 2.6, 2.4, 2.1, 1.9
    ];
    let age_this_year = current_year - birthday.year();

    Some(match age_this_year {
        // use u64::TryFrom()
        x @ 70 if birthday.month() < 7 => distribution_periods[(x - 70) as usize],
        x @ 71 ... 115 => distribution_periods[(x - 70) as usize],
        x if x >= 115 => distribution_periods[115 - 70],
        _ => return None,
    })
}

fn get_rmd(birthday: NaiveDate, now: NaiveDate, prior_year_ending_ira_value: u64) -> u64 {
    if let Some(distribution_period) = get_rmd_distribution_period(birthday, now.year()) {
        ((prior_year_ending_ira_value as f64) / distribution_period) as u64
    } else {
        0
    }
}

// Tax tables: https://taxmap.irs.gov/taxmap/ts0/taxtable_o_03b62156.htm
// 2019 Tax Rate Schedule: https://www.irs.gov/pub/irs-prior/f1040es--2019.pdf#page=7
// TODO: AMT?
pub fn get_tax(taxable_income: u64) -> u64 {
    // TODO: applies to single filing status only
    (match taxable_income as f64 {
     x if x > 510_300f64 => 0.37 * (x - 510_300f64) + 153_798.50,
     x if x > 204_100f64 => 0.35 * (x - 204_100f64) + 46_628.50,
     x if x > 160_725f64 => 0.32 * (x - 160_725f64) + 32_748.50,
     x if x > 84_200f64 => 0.24 * (x - 84_200f64) + 14_382.50,
     x if x > 39_475f64 => 0.22 * (x - 39_475f64) + 4_543.00,
     x if x > 9_700f64 => 0.12 * (x - 9_700f64) + 970.00,
     x if x > 0f64 => 0.10 * x,
     _ => 0f64,
    }) as u64
}

fn to_continuous_compound_rate(effective_annual_rate: f64) -> f64 {
    let n = 1_f64;
    n * (effective_annual_rate/n).ln_1p()
}

fn compound(current_value: f64, rate: f64, years: f64) -> f64 {
    use std::f64::consts::E;
    current_value * E.powf(rate * years)
}

#[cfg(test)]
mod tests {
    use super::*;
    use test::Bencher;

    #[test]
    fn rmd_distribution_period_turns_70_june_30() {
        assert_eq!(Some(27.4), get_rmd_distribution_period(NaiveDate::from_ymd(1949, 6, 30), 2019));
    }

    #[test]
    fn rmd_distribution_period_turns_71_june_30() {
        assert_eq!(Some(26.5), get_rmd_distribution_period(NaiveDate::from_ymd(1948, 6, 30), 2019));
    }

    #[test]
    fn rmd_distribution_period_turns_70_july_1() {
        assert_eq!(None, get_rmd_distribution_period(NaiveDate::from_ymd(1949, 7, 1), 2019));
    }

    #[test]
    fn rmd_distribution_period_turns_71_july_1() {
        assert_eq!(Some(26.5), get_rmd_distribution_period(NaiveDate::from_ymd(1948, 7, 1), 2019));
    }

    #[test]
    #[ignore]
    fn long_project() {
        assert!(project(ProjectArgs {
            yearly_taxable_income_excluding_ira: 10000,
            inflation_effective_annual_rate: 0.03,
            roth_present_value: 5000,
            roth_effective_annual_rate: 0.08,
            ira_present_value: 6000,
            ira_effective_annual_rate: 0.08,
            birthday: NaiveDate::from_ymd(1955, 6, 3),
            end_date: NaiveDate::from_ymd(2040, 12, 31),
            now: NaiveDate::from_ymd(2019, 12, 31),
        }).is_some());
    }

    #[test]
    fn short_project() {
        assert!(project(ProjectArgs {
            yearly_taxable_income_excluding_ira: 10000,
            inflation_effective_annual_rate: 0.03,
            roth_present_value: 5000,
            roth_effective_annual_rate: 0.08,
            ira_present_value: 6000,
            ira_effective_annual_rate: 0.08,
            birthday: NaiveDate::from_ymd(1955, 6, 3),
            end_date: NaiveDate::from_ymd(2040, 12, 31),
            // O(2^n) scaling, n = end_year - now_year
            now: NaiveDate::from_ymd(2035, 12, 31),
        }).is_some());
    }

    #[bench]
    fn short_project_bench(b: &mut Bencher) {
        b.iter(|| short_project());
    }
}
