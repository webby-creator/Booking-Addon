#[macro_use]
extern crate tracing;

use std::net::SocketAddr;

use addon_common::{request::query_cms_rows, JsonResponse, WrappingResponse};
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use eyre::ContextCompat;
use global_common::{
    request::CmsQuery,
    response::CmsRowResponse,
    schema::SchematicFieldKey,
    uuid::{CollectionName, UuidType},
};
use time::{
    macros::{format_description, offset},
    Duration, PrimitiveDateTime, UtcOffset,
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
            .route("/:uuid/available", get(get_available))
            // .route("/:uuid/book", post(post_booking))
            .layer(TraceLayer::new_for_http()),
    )
    .await?;

    Ok(())
}

#[derive(serde::Deserialize)]
struct GetAvailableQuery {
    year: usize,
    month: u8,
    day: Option<usize>,
}

#[derive(serde::Deserialize)]
struct RecurrenceRule {
    days: Vec<String>,
    frequency: String,
    interval: usize,
}

async fn get_available(
    Path(uuid): Path<UuidType>,
    Query(query): Query<GetAvailableQuery>,
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

    // TODO: If day is specified, return available hours in the day

    let available_days = gather_available_days(now, staff_schedule_resp.items)?;

    // TODO: Simplify
    Ok(Json(WrappingResponse::okay(serde_json::json!({
        "timeZone": "America/Los_Angeles",
        "availableDays": available_days
    }))))
}

fn gather_available_days(
    now: PrimitiveDateTime,
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
        let local_offset = offset!(-7);

        let curr_dt = start_date
            .with_time(start_time)
            // America/Los_Angeles PST (UTC-7)
            .assume_offset(local_offset)
            .to_offset(UtcOffset::UTC);

        {
            // TODO: Interval
            let freq = frequency_str_to_duration(&rec_rule.frequency)?;

            let mut found = Vec::new();

            let mut pos = curr_dt.clone();

            // TODO: Can be improved. This is a brute force method.
            loop {
                pos = pos.saturating_add(freq);

                // If we're in the current month, we can add it to the list.
                if now.month() == pos.month() {
                    found.push(pos);

                    // There should only ever be a MAX of 5 weeks in a month.
                    // If we reach 5, we can stop to prevent another loop.
                    if found.len() == 5 {
                        break;
                    }
                }
                // If we passed the current month, we can stop.
                else if pos.month() as u8 > now.month() as u8 {
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
