pub mod create;
pub mod delete;
pub mod delete_many;
pub mod list;
pub mod merge;
pub mod update;

fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}
