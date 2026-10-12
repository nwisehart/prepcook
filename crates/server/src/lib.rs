pub mod error;
pub mod middleware;
pub mod routes;
pub mod startup;

// #[cfg(feature = "cloud")]
// type DeploymentImpl = prepcook_cloud::deployment::CloudDeployment;
// #[cfg(not(feature = "cloud"))]
pub type DeploymentImpl = local_deployment::LocalDeployment;
