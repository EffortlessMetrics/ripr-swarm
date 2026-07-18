use serde::Serialize;

#[derive(Serialize)]
pub struct DoctorReport {
    pub schema_version: String,
    pub tool: String,
    pub kind: String,
    pub checks: Vec<DoctorCheck>,
    pub overall: String,
}

#[derive(Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
    pub recovery_route: Option<String>,
}
