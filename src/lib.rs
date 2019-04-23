use pathfinding::prelude::astar;
use chrono::Duration;
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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct State {
    adjusted_spendable_income: u64,
    now: NaiveDate,
    roth_present_value: u64,
    ira_present_value: u64,
}

type Cost = u64;

impl State {
    fn step_time(&self, args: &ProjectArgs) -> Result<(State, Cost), Error> {
        let now = self.now.checked_add_signed(Duration::days(365)).ok_or_else(|| err_msg("overflow"))?;
        let ira_value = ((self.ira_present_value as f64) * (1f64 + args.ira_effective_annual_rate - args.inflation_effective_annual_rate)) as u64;
        let ira_rmd = get_rmd(args.birthday, now, ira_value)?;

        let roth_value = ((self.roth_present_value as f64) * (1f64 + args.roth_effective_annual_rate - args.inflation_effective_annual_rate)) as u64;

        let taxable_income = args.yearly_taxable_income_excluding_ira + ira_rmd;
        let tax = get_tax(taxable_income);

        Ok((State {
            adjusted_spendable_income: self.adjusted_spendable_income + taxable_income,
            roth_present_value: roth_value,
            ira_present_value: ira_value - ira_rmd,
            now: now,
        }, tax)) 
    }

    fn successors(&self, args: &ProjectArgs) -> Result<Vec<(State, Cost)>, Error> {
        Ok(if self.now >= args.end_date {
            vec![]
        } else {
            vec![
                self.step_time(args)?
            ]
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
        // TODO: Pass in from args instead, so tests are reproducible
        now: args.now,
        roth_present_value: args.roth_present_value,
        ira_present_value: args.ira_present_value,
    };

    astar(&start,
          |ref s| s.successors(&args).unwrap(),
          // TODO: improve
          |_| get_tax(args.yearly_taxable_income_excluding_ira),
          |ref s| dbg!(s).now >= args.end_date,
          )
}

pub fn get_rmd(birthday: NaiveDate, now: NaiveDate, ira_value: u64) -> Result<u64, Error> {
    let age = now.signed_duration_since(birthday);
    let age_above_thresh = age.checked_sub(&Duration::days((365.0*74.5) as i64)).ok_or_else(|| err_msg("Overflow"))?;

    // TODO: https://www.irs.gov/pub/irs-tege/uniform_rmd_wksht.pdf
    Ok(if age_above_thresh.num_days() > 0 {
        (ira_value as f64 * 0.1) as u64
    } else {
        0
    })
}

// TODO: tax tables, etc: https://taxmap.irs.gov/taxmap/ts0/taxtable_o_03b62156.htm
pub fn get_tax(taxable_income: u64) -> u64 {
    (taxable_income as f64 * 0.1) as u64
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

    #[test]
    fn it_works() {
        assert!(project(ProjectArgs {
            // TODO: make Vec
            yearly_taxable_income_excluding_ira: 10000,
            inflation_effective_annual_rate: 0.03,
            roth_present_value: 5000,
            roth_effective_annual_rate: 0.08,
            ira_present_value: 6000,
            ira_effective_annual_rate: 0.08,
            birthday: NaiveDate::from_ymd(1960, 6,  3),
            end_date: NaiveDate::from_ymd(2030, 6,  3),
            now: NaiveDate::from_ymd(2019, 4, 22),
        }).is_some());
    }
}
