//#[wasm_bindgen]
//pub fn add(a: u64, b: u64) -> u64 {
//    a + b
//}
//
//
//// TODO: tax tables, etc: https://taxmap.irs.gov/taxmap/ts0/taxtable_o_03b62156.htm

pub struct ProjectArgs {
    yearly_taxable_income_excluding_ira: u64,
    inflation_effective_annual_rate: f64,
    //roth_present_value: u64,
    //roth_effective_annual_rate: f64,
    ira_present_value: u64,
    ira_effective_annual_rate: f64,
    //birthday: Date,
}

pub fn project(args: ProjectArgs) {
    // IRA RMD: https://www.irs.gov/pub/irs-tege/uniform_rmd_wksht.pdf
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
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
