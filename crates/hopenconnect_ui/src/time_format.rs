use time::OffsetDateTime;

pub(crate) fn format_unix_seconds(value: &str) -> Option<String> {
    let seconds = value.trim().parse::<i64>().ok()?;
    let datetime = OffsetDateTime::from_unix_timestamp(seconds).ok()?;
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        datetime.year(),
        u8::from(datetime.month()),
        datetime.day(),
        datetime.hour(),
        datetime.minute()
    ))
}
