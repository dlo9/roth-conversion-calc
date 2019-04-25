#![feature(test)]
extern crate test;

#[macro_use]
extern crate lazy_static;

use pathfinding::prelude::astar;
use failure::*;
use std::convert::TryFrom;

pub struct ProjectArgs {
    // TODO: make Vec
    yearly_taxable_income_excluding_ira: u64,
    inflation_effective_annual_rate: f64,
    roth_present_value: u64,
    roth_effective_annual_rate: f64,
    ira_present_value: u64,
    ira_effective_annual_rate: f64,
    birth_year: u16,
    birth_month: u8,
    // TODO: these dates should ALWAYS be Dec 31. 
    start_year: u16,
    end_year: u16,
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
        } else if self.birth_year > self.start_year {
            err_msg("Birth year must be <= start year")
        } else if self.start_year > self.end_year {
            err_msg("End year must be >= start year")
        // TODO: range.contains once stable: https://doc.rust-lang.org/std/ops/struct.Range.html#method.contains
        } else if self.birth_month < 1 || self.birth_month > 12 {
            err_msg("Birth month must be between 1 and 12")
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
    current_year: u16,
    roth_present_value: u64,
    ira_present_value: u64,
}

type Cost = u64;

struct Successors {
    time: Option<(State, Cost)>,
    rollover: Option<(State, Cost)>,
}

impl Successors {
    pub fn new(parent: &State, args: &ProjectArgs) -> Successors {
        Successors {
            time: parent.step_year(args).ok(),
            rollover: parent.step_rollover(1000),
        }
    }
}

impl Iterator for Successors {
    type Item = (State, Cost);

    fn next(&mut self) -> Option<Self::Item> {
        let mut ret = None;

        if self.time.is_some() {
            std::mem::swap(&mut ret, &mut self.time);
        }

        if ret.is_none() && self.rollover.is_some() {
            std::mem::swap(&mut ret, &mut self.rollover);
        }

        ret
    }
}

impl State {
    fn step_year(&self, args: &ProjectArgs) -> Result<(State, Cost), Error> {
        // TODO: is the rollover & RMD meshing properly?
        let ira_rmd = get_rmd(args.birth_year, args.birth_month, self.current_year, self.ira_present_value).checked_sub(self.pending_rollover).unwrap_or_default();
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
            current_year: self.current_year + 1,
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
}

// TODO: #[wasm_bindgen]
pub fn project(args: &ProjectArgs) -> Option<(Vec<State>, Cost)> {
    if args.validate().is_err() {
        return None;
    }

    let start = State {
        adjusted_spendable_income: 0,
        pending_rollover: 0,
        // TODO: Pass in from args instead, so tests are reproducible
        current_year: args.start_year,
        roth_present_value: args.roth_present_value,
        ira_present_value: args.ira_present_value,
    };

    dbg!(astar(&start,
               |ref s| Successors::new(s, args),
               // TODO: improve
               |ref s| get_tax(args.yearly_taxable_income_excluding_ira + s.pending_rollover),
               |ref s| s.current_year >= args.end_year,
               ))
}

// TODO: only applies if (spouse not sole beneficiary) || (their age >= your age - 10)
// Worksheet: https://www.irs.gov/pub/irs-tege/uniform_rmd_wksht.pdf
fn get_rmd_distribution_period(birth_year: u16, birth_month: u8, current_year: u16) -> Option<f64> {
    lazy_static! {
        // Index 0 == age 70
        static ref DISTRIBUTION_PERIODS: [f64; 46] = [
            27.4, 26.5, 25.6, 24.7, 23.8, 22.9, 22.0, 21.2, 20.3, 19.5, 18.7, 17.9,
            17.1, 16.3, 15.5, 14.8, 14.1, 13.4, 12.7, 12.0, 11.4, 10.8, 10.2, 9.6,
            9.1, 8.6, 8.1, 7.6, 7.1, 6.7, 6.3, 5.9, 5.5, 5.2, 4.9, 4.5,
            4.2, 3.9, 3.7, 3.4, 3.1, 2.9, 2.6, 2.4, 2.1, 1.9
        ];
    }

    let age_this_year = current_year.checked_sub(birth_year).unwrap_or_default();
    // TODO: try_from still necessary now that I'm using u16?
    Some(match usize::try_from(age_this_year).unwrap_or_default() {
        x @ 70 if birth_month < 7 => DISTRIBUTION_PERIODS[x - 70],
        x @ 71 ... 115 => DISTRIBUTION_PERIODS[x - 70],
        x if x >= 115 => DISTRIBUTION_PERIODS[115 - 70],
        _ => return None,
    })
}

fn get_rmd(birth_year: u16, birth_month: u8, year: u16, prior_year_ending_ira_value: u64) -> u64 {
    if let Some(distribution_period) = get_rmd_distribution_period(birth_year, birth_month, year) {
        ((prior_year_ending_ira_value as f64) / distribution_period) as u64
    } else {
        0
    }
}

// Tax tables: https://taxmap.irs.gov/taxmap/ts0/taxtable_o_03b62156.htm
// 2019 Tax Rate Schedule: https://www.irs.gov/pub/irs-prior/f1040es--2019.pdf#page=7
// TODO: AMT?
// TODO: applies to single filing status only (make FilingStatus a trait with req'd fn figure_tax)
fn get_tax(taxable_income: u64) -> u64 {
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
        assert_eq!(Some(27.4), get_rmd_distribution_period(1949, 6, 2019));
    }

    #[test]
    fn rmd_distribution_period_turns_71_june_30() {
        assert_eq!(Some(26.5), get_rmd_distribution_period(1948, 6, 2019));
    }

    #[test]
    fn rmd_distribution_period_turns_70_july_1() {
        assert_eq!(None, get_rmd_distribution_period(1949, 7, 2019));
    }

    #[test]
    fn rmd_distribution_period_turns_71_july_1() {
        assert_eq!(Some(26.5), get_rmd_distribution_period(1948, 7, 2019));
    }

    #[test]
    fn rmd_distribution_period_age_butween_70_and_115() {
        assert_eq!(Some(11.4), get_rmd_distribution_period(2019 - 90, 3, 2019));
    }

    #[test]
    fn rmd_distribution_period_age_115() {
        assert_eq!(Some(1.9), get_rmd_distribution_period(2019 - 115, 3, 2019));
    }

    #[test]
    fn rmd_distribution_period_age_greater_than_115() {
        assert_eq!(Some(1.9), get_rmd_distribution_period(2019 - 116, 3, 2019));
    }

    #[test]
    fn rmd_distribution_period_age_less_than_70() {
        assert_eq!(None, get_rmd_distribution_period(2019 - 69, 3, 2019));
    }

    #[test]
    fn rmd_distribution_period_negative_age() {
        assert_eq!(None, get_rmd_distribution_period(2019 + 1, 3, 2019));
    }

    #[test]
    fn tax_gt_510_300() {
        assert_eq!(153835, get_tax(510_400));
    }

    #[test]
    fn tax_0() {
        assert_eq!(0, get_tax(0));
    }

    #[bench]
    #[ignore]
    fn long_project(b: &mut Bencher) {
        let args = ProjectArgs {
            yearly_taxable_income_excluding_ira: 10000,
            inflation_effective_annual_rate: 0.03,
            roth_present_value: 5000,
            roth_effective_annual_rate: 0.08,
            ira_present_value: 6000,
            ira_effective_annual_rate: 0.08,
            birth_year: 1955,
            birth_month: 6,
            start_year: 2019,
            end_year: 2040,
        };

        b.iter(|| assert!(project(&args).is_some()));
    }

    #[bench]
    fn short_project(b: &mut Bencher) {
        let args = ProjectArgs {
            yearly_taxable_income_excluding_ira: 10000,
            inflation_effective_annual_rate: 0.03,
            roth_present_value: 5000,
            roth_effective_annual_rate: 0.08,
            ira_present_value: 6000,
            ira_effective_annual_rate: 0.08,
            birth_year: 1955,
            birth_month: 6,
            start_year: 2035,
            end_year: 2040,
        };

        b.iter(|| assert!(project(&args).is_some()));
    }
}
