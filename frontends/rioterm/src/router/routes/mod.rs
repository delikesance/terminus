pub mod assistant;
pub mod updating;
pub mod welcome;

#[derive(PartialEq)]
pub enum RoutePath {
    Terminal,
    Welcome,
    /// Installing an update found at launch (see `crate::updater`).
    Updating,
}
