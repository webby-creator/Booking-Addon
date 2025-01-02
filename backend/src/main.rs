#[macro_use]
extern crate tracing;

use std::net::SocketAddr;

use addon_common::{
    request::{get_cms_row_by_id, query_cms_rows},
    JsonResponse, WrappingResponse,
};
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use eyre::ContextCompat;
use global_common::{
    filter::{Filter, FilterConditionType, FilterValue},
    request::CmsQuery,
    response::CmsRowResponse,
    schema::SchematicFieldKey,
    tz::find_offset_by_id,
    uuid::{CollectionName, UuidType},
};
use time::{
    format_description::well_known::Iso8601, macros::format_description, Duration,
    PrimitiveDateTime, UtcOffset,
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

mod error;

pub use error::{Error, Result};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    let port = 5941;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    debug!("Addon Booking listening on {addr}");

    let listener = TcpListener::bind(addr).await.unwrap();

    axum::serve(
        listener,
        Router::new()
            .route("/:uuid/availableDays", get(get_available_days))
            .route("/:uuid/availableHours", get(get_available_hours))
            // .route("/:uuid/book", post(post_booking))
            .layer(TraceLayer::new_for_http()),
    )
    .await?;

    Ok(())
}

// TODO: Account for start time being larger than end time.
// Eg: Start 11pm -> End 5am

#[derive(serde::Deserialize)]
struct GetAvailableDaysQuery {
    year: usize,
    month: u8,
}

#[derive(serde::Deserialize)]
struct RecurrenceRule {
    days: Vec<String>,
    frequency: String,
    interval: usize,
}

async fn get_available_days(
    Path(uuid): Path<UuidType>,
    Query(query): Query<GetAvailableDaysQuery>,
) -> Result<JsonResponse<serde_json::Value>> {
    // TODO: Remember Daylight Savings Time

    let now =
        time::Date::from_calendar_date(query.year as i32, time::Month::try_from(query.month)?, 1)?
            .midnight();

    let staff_schedule_resp = query_cms_rows(
        uuid,
        CollectionName {
            id: String::from("staffSchedule"),
            ns: Some(String::from("@booking")),
        },
        CmsQuery {
            filters: None,
            // filters: Some(vec![Filter {
            //     name: String::from("startDay"),
            //     cond: FilterConditionType::Lte,
            //     value: FilterValue::Text(String::from("2024-12-12")),
            // }]),
            sort: None,
            columns: None,
            limit: None,
            offset: None,
            include_files: false,
        },
    )
    .await?;

    // Example Schedule:
    // 2024-12-06 | 10:00:00 - 18:00:00 America/Los_Angeles | 1/wk
    // Fridays: Nov 1, 8, 15, 22, 29 | Dec 6, 13, 20, 27

    let available_days = gather_available_days(now, staff_schedule_resp.items)?;

    // TODO: Simplify
    Ok(Json(WrappingResponse::okay(serde_json::json!({
        "available": available_days,
    }))))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetAvailableHoursQuery {
    day: u8,
    month: u8,
    year: usize,
    schedule_ids: String,
}

async fn get_available_hours(
    Path(uuid): Path<UuidType>,
    Query(GetAvailableHoursQuery {
        day,
        month,
        year,
        schedule_ids,
    }): Query<GetAvailableHoursQuery>,
) -> Result<JsonResponse<serde_json::Value>> {
    // TODO: Remember Daylight Savings Time

    let list_date =
        time::Date::from_calendar_date(year as i32, time::Month::try_from(month)?, day)?.midnight();

    let mut staff_schedule = get_cms_row_by_id(
        uuid,
        CollectionName {
            id: String::from("staffSchedule"),
            ns: Some(String::from("@booking")),
        },
        // TODO: Multiple ids can be in here.
        &schedule_ids,
    )
    .await?;

    let schedule = get_cms_row_by_id(
        uuid,
        CollectionName {
            id: String::from("schedule"),
            ns: Some(String::from("@booking")),
        },
        &staff_schedule
            .fields
            .get(&SchematicFieldKey::Other(String::from("schedule")))
            .context("Schedule ID")?
            .any_as_text()?,
    )
    .await?;

    let service = get_cms_row_by_id(
        uuid,
        CollectionName {
            id: String::from("services"),
            ns: Some(String::from("@booking")),
        },
        &schedule
            .fields
            .get(&SchematicFieldKey::Other(String::from("service")))
            .context("Service ID")?
            .any_as_text()?,
    )
    .await?;

    let bookings = query_cms_rows(
        uuid,
        CollectionName {
            id: String::from("bookings"),
            ns: Some(String::from("@booking")),
        },
        CmsQuery {
            filters: Some(vec![
                Filter {
                    name: String::from("bookDate"),
                    cond: FilterConditionType::Gte,
                    value: FilterValue::Text(format!(
                        "{year}-{month:02}-{day:02} 00:00:00.0 +00:00:00"
                    )),
                },
                Filter {
                    name: String::from("bookDate"),
                    cond: FilterConditionType::Lte,
                    value: FilterValue::Text(format!(
                        "{year}-{month:02}-{day:02} 23:59:59.0 +00:00:00"
                    )),
                },
            ]),
            // sort: None,
            // columns: None,
            // limit: None,
            // offset: None,
            // include_files: false,
            ..CmsQuery::default()
        },
    )
    .await?;

    let time_zone_str = staff_schedule
        .fields
        .get(&SchematicFieldKey::Other(String::from("timeZone")))
        .cloned()
        .context("Missing TimeZone")?
        .try_as_text()?;
    let local_offset = find_offset_by_id(&time_zone_str).context("Invalid TimeZone")?;

    let booked_times = bookings
        .items
        .iter()
        .map(|item| {
            let start_time = item
                .fields
                .get(&SchematicFieldKey::Other(String::from("bookDate"))).unwrap()
                .any_as_text().unwrap();

            // Parse start_time value of 2025-01-02 12:00:00.0 +00:00:00
            let start_time = time::OffsetDateTime::parse(
                &start_time,
                &format_description!(
                    "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond] [offset_hour sign:mandatory]:[offset_minute]:[offset_second]"
                ),
            )
            .unwrap()
            .replace_offset(local_offset);

            start_time
        })
        .collect::<Vec<_>>();

    // println!("{bookings:#?}");
    // println!("{booked_times:?}");

    // TODO: Get bookings for the start-end time for the staff schedule(s)

    let duration = Duration::minutes(
        schedule
            .fields
            .get(&SchematicFieldKey::Other(String::from("duration")))
            .context("Service Duration")?
            .try_as_number()?
            .convert_f64() as i64,
    );

    let break_duration = Duration::minutes(
        schedule
            .fields
            .get(&SchematicFieldKey::Other(String::from("break")))
            .context("Service Break")?
            .try_as_number()?
            .convert_f64() as i64,
    );

    // schedule.fields.get(&SchematicFieldKey::Other(String::from("serviceSchedule"))) (not used yet)
    // schedule.fields.get(&SchematicFieldKey::Other(String::from("repeats")))

    // service.fields.get(&SchematicFieldKey::Other(String::from("maxParticipants")))
    // service.fields.get(&SchematicFieldKey::Other(String::from("priceAmount")))
    // service.fields.get(&SchematicFieldKey::Other(String::from("paymentType")))
    // service.fields.get(&SchematicFieldKey::Other(String::from("name")))
    // service.fields.get(&SchematicFieldKey::Other(String::from("type")))

    // println!("{staff_schedule:#?}");
    // println!("{service:#?}");

    let mut available_hours = Vec::new();

    {
        // let date_format = format_description!("[year]-[month]-[day]");
        let time_format = format_description!("[hour]:[minute]:[second]");

        // let start_date = staff_schedule
        //     .fields
        //     .remove(&SchematicFieldKey::Other(String::from("startDay")))
        //     .context("Missing startDay field")?;

        // TODO: We're currently assuming that start time < end time. We need to add a date to both of them.
        let start_time = staff_schedule
            .fields
            .remove(&SchematicFieldKey::Other(String::from("start")))
            .context("Missing start field")?
            .try_as_text()?
            .replace(".0", "");

        let end_time = staff_schedule
            .fields
            .remove(&SchematicFieldKey::Other(String::from("end")))
            .context("Missing end field")?
            .try_as_text()?
            .replace(".0", "");

        // let start_date = time::Date::parse(&start_date.try_as_text()?, &date_format).unwrap();
        let start_time = time::Time::parse(&start_time, &time_format).unwrap();
        let end_time = time::Time::parse(&end_time, &time_format).unwrap();

        // We don't convert to UTC since start_time & end_time is in local offset time.
        let mut current_time_pos = list_date
            .replace_time(start_time)
            .assume_offset(local_offset);

        // Loop until we hit the end of time
        while current_time_pos.time() + duration + break_duration <= end_time {
            // TODO: Replace w/ UTC offset temporarily to fix JavaScript Date
            let utc_time_pos = current_time_pos.replace_offset(UtcOffset::UTC);

            available_hours.push(serde_json::json!({
                "start": utc_time_pos.format(&Iso8601::DEFAULT).unwrap(),
                "end": (utc_time_pos + duration).format(&Iso8601::DEFAULT).unwrap(),
                "isBooked": booked_times.iter().any(|booked_time| {
                    *booked_time >= current_time_pos && *booked_time <= current_time_pos + duration
                }),
            }));

            current_time_pos += duration + break_duration;
        }
    }

    Ok(Json(WrappingResponse::okay(serde_json::json!({
        "timeZone": time_zone_str,
        "available": available_hours,
    }))))
}

fn gather_available_days(
    lookup_time: PrimitiveDateTime,
    staff_schedule_items: Vec<CmsRowResponse>,
) -> Result<Vec<serde_json::Value>> {
    let mut available_days = Vec::new();

    // 1st. Convert Date/Time to UTC
    let date_format = format_description!("[year]-[month]-[day]");
    let time_format = format_description!("[hour]:[minute]:[second]");

    for mut item in staff_schedule_items {
        let start_date = item
            .fields
            .remove(&SchematicFieldKey::Other(String::from("startDay")))
            .context("Missing startDay field")?;

        let start_time = item
            .fields
            .remove(&SchematicFieldKey::Other(String::from("start")))
            .context("Missing start field")?
            .try_as_text()?
            .replace(".0", "");

        let end_time = item
            .fields
            .remove(&SchematicFieldKey::Other(String::from("end")))
            .context("Missing end field")?
            .try_as_text()?
            .replace(".0", "");

        let rec_rule: RecurrenceRule = serde_json::from_value(serde_json::to_value(
            item.fields
                .remove(&SchematicFieldKey::Other(String::from("recurrenceRule")))
                .context("Missing end field")?,
        )?)?;

        let start_date = time::Date::parse(&start_date.try_as_text()?, &date_format).unwrap();
        let start_time = time::Time::parse(&start_time, &time_format).unwrap();
        let end_time = time::Time::parse(&end_time, &time_format).unwrap();

        let time_distance = end_time - start_time;

        // TODO: Remove Hardcoding
        let time_zone_str = item
            .fields
            .get(&SchematicFieldKey::Other(String::from("timeZone")))
            .cloned()
            .context("Missing TimeZone")?
            .try_as_text()?;
        let local_offset = find_offset_by_id(&time_zone_str).context("Invalid TimeZone")?;

        let curr_dt = start_date
            .with_time(start_time)
            // America/Los_Angeles PST (UTC-7)
            .assume_offset(local_offset)
            .to_offset(UtcOffset::UTC);

        // TODO: Interval
        let freq = frequency_str_to_duration(&rec_rule.frequency)?;

        let mut found = Vec::new();

        let mut pos = curr_dt.clone();

        // TODO: Can be improved. This is a brute force method.
        loop {
            pos = pos.saturating_add(freq);

            // If we're in the current month, we can add it to the list.
            if lookup_time.month() == pos.month() {
                found.push(pos);

                // There should only ever be a MAX of 5 weeks in a month.
                // If we reach 5, we can stop to prevent another loop.
                if found.len() == 5 {
                    break;
                }
            }
            // If we passed the current month, we can stop.
            else if pos.month() as u8 > lookup_time.month() as u8 {
                break;
            }
            // If we're still in the past, we can skip.
            else if found.is_empty() {
                continue;
            } else {
                break;
            }
        }

        for utc in found {
            let local = utc.to_offset(local_offset);

            // Start DateTime ID
            // TODO: Chars [32 start time][1 version][3 duration][1 recurrence][3 original utc offset]
            let start_id = Uuid::new_v7(uuid::Timestamp::from_unix(
                uuid::NoContext,
                utc.unix_timestamp() as u64,
                0,
            ));

            available_days.push(serde_json::json!({
                // TODO: Add Duration, Recurrence, Week Day, etc.. to it.
                "id": start_id.as_simple(),
                "staffScheduleId": item.fields.get(&SchematicFieldKey::Id).unwrap(),
                "timeZone": time_zone_str,

                "start": {
                    "dateUtc": utc.date(),
                    "timeUtc": utc.time().format(&time_format).unwrap(),
                    "dateLocal": local.date(),
                    "timeLocal": start_time.format(&time_format).unwrap(),
                },

                "end": {
                    "dateUtc": (utc + time_distance).date(),
                    "timeUtc": (utc.time() + time_distance).format(&time_format).unwrap(),
                    "dateLocal": (local + time_distance).date(),
                    "timeLocal": end_time.format(&time_format).unwrap(),
                },

                "monthUtc": utc.month() as u8,
                "dayUtc": utc.day() as u8,

                "monthLocal": local.month() as u8,
                "dayLocal": local.day() as u8,
            }));
        }
    }

    Ok(available_days)
}

fn frequency_str_to_duration(frequency: &str) -> Result<Duration> {
    Ok(match frequency {
        "DAILY" => Duration::days(1),
        "WEEKLY" => Duration::weeks(1),
        "MONTHLY" => Duration::weeks(4),
        "YEARLY" => Duration::weeks(52),
        v => return Err(eyre::eyre!("Invalid frequency: {v}"))?,
    })
}

// Start DateTime ID
// TODO: Chars [32 start time][1 version][3 duration][1 recurrence][3 original utc offset]
struct BookingId {
    /// Stores the DateTime in which the booking starts in UTC format.
    start_time: Uuid,
    /// Version of the BookingID
    version: u8,
    /// The duration of the booking in minutes.
    duration: u32,
    /// The recurrence of the booking.
    recurrence: u8,
    /// The original offset for the start time.
    /// Since its' currently stored as UTC.
    utc_offset: u32,
}

// let start_id = Uuid::new_v7(uuid::Timestamp::from_unix(
//     uuid::NoContext,
//     utc.unix_timestamp() as u64,
//     0,
// ));

// impl serde::Serialize for BookingId {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: serde::Serializer,
//     {
//         let mut this = self.start_time.simple().to_string();

//         // format!("{:X}", 42)
//         // i64::from_str_radix("1f", 16);

//         // self.version
//         // self.duration
//         // self.recurrence
//         // self.utc_offset

//         Ok(this)
//     }
// }
