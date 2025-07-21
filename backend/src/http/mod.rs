use std::collections::HashMap;

use addon_common::{
    request::{
        create_cms_collection, create_website_form, create_website_form_action, CreateWebsiteForm,
        FormAction, FormActionEmail, FormFieldType, FormType, Layer, LayerInput, LayerInputData,
        LayerRow,
    },
    InstallResponse, JsonResponse, RegisterNewJson, WrappingResponse,
};
use axum::{routing::post, Json, Router};
use eyre::ContextCompat;
use global_common::{
    request::{CmsCreate, CmsCreateDataColumn, CmsUpdate},
    schema::SchematicFieldType,
    uuid::CollectionName,
    value::{Number, SimpleValue},
};
use time::macros::format_description;
use uuid::Uuid;

use crate::Result;

pub fn routes() -> Router<()> {
    Router::new().route("/", post(post_install))
}

async fn post_install(
    Json(RegisterNewJson {
        instance_id,
        website_id,
        owner_id,
        member,
        website,
        version,
    }): Json<RegisterNewJson>,
) -> Result<JsonResponse<InstallResponse>> {
    let date_format = format_description!("[year]-[month]-[day]");
    let time_format = format_description!("[hour]:[minute]:[second].[subsecond]");

    // TODO: Ability to wrap requests in a "transaction".
    // Send a the same unique x-transaction-id header with each request.r
    // Store each master copy id w/ ability to delete everything if it fails.

    let mut index = 0;

    fn gen_id(field_type: FormFieldType, index: &mut usize) -> String {
        *index += 1;

        format!("{}{index}", field_type.to_string())
    }

    let form = create_website_form(
        website_id,
        CreateWebsiteForm {
            name: Some(String::from("Haircut Service")),
            type_of: FormType::Contact,
            addon_uuid: Some(Uuid::from_u128(0x01938f4ff50c72039f89b367e9d49efbu128)),
            layers: Some(vec![Layer {
                id: String::new(),
                name: None,
                rows: vec![
                    LayerRow {
                        id: String::new(),
                        items: vec![
                            LayerInput::Input {
                                id: gen_id(FormFieldType::FirstName, &mut index),
                                key: Some(String::from("firstName")),
                                type_of: FormFieldType::FirstName,
                                offset: 0,
                                size: 8,
                                data: LayerInputData::Text {
                                    hidden: false,
                                    required: true,
                                    field_title: None,
                                    placeholder_text: Some(String::from("First Name")),
                                    field_description: None,
                                    personal_info: false,
                                    limit: Some((Number::Integer(0), Number::Integer(30))),
                                },
                            },
                            LayerInput::Input {
                                id: gen_id(FormFieldType::LastName, &mut index),
                                key: Some(String::from("lastName")),
                                type_of: FormFieldType::LastName,
                                offset: 8,
                                size: 8,
                                data: LayerInputData::Text {
                                    hidden: false,
                                    required: true,
                                    field_title: None,
                                    placeholder_text: Some(String::from("Last Name")),
                                    field_description: None,
                                    personal_info: false,
                                    limit: Some((Number::Integer(0), Number::Integer(30))),
                                },
                            },
                        ],
                    },
                    LayerRow {
                        id: String::new(),
                        items: vec![
                            LayerInput::Input {
                                id: gen_id(FormFieldType::Email, &mut index),
                                key: Some(String::from("email")),
                                type_of: FormFieldType::Email,
                                offset: 0,
                                size: 8,
                                data: LayerInputData::Text {
                                    hidden: false,
                                    required: true,
                                    field_title: None,
                                    placeholder_text: Some(String::from("Email Address")),
                                    field_description: None,
                                    personal_info: false,
                                    limit: Some((Number::Integer(0), Number::Integer(60))),
                                },
                            },
                            LayerInput::Input {
                                id: gen_id(FormFieldType::Phone, &mut index),
                                key: Some(String::from("phone")),
                                type_of: FormFieldType::Phone,
                                offset: 8,
                                size: 8,
                                data: LayerInputData::Text {
                                    hidden: false,
                                    required: false,
                                    field_title: None,
                                    placeholder_text: Some(String::from("Phone Number")),
                                    field_description: None,
                                    personal_info: false,
                                    limit: None,
                                },
                            },
                        ],
                    },
                    LayerRow {
                        id: String::new(),
                        items: vec![LayerInput::Input {
                            id: gen_id(FormFieldType::LongText, &mut index),
                            key: Some(String::from("message")),
                            type_of: FormFieldType::LongText,
                            offset: 0,
                            size: 16,
                            data: LayerInputData::Text {
                                hidden: false,
                                required: false,
                                field_title: None,
                                placeholder_text: Some(String::from("Message")),
                                field_description: None,
                                personal_info: false,
                                limit: None,
                            },
                        }],
                    },
                ],
            }]),
            conditions: None,
        },
    )
    .await?;

    create_website_form_action(
        website_id,
        form.id,
        FormAction::Email(FormActionEmail {
            subject: String::from("You received a new booking for {{bookingDateTime}}!"),
            // TODO: Replace w/ String::from("{{OWNER_EMAIL}}")
            send_to: vec![member.email.clone().context("Member Email")?],
            from_name: member.email.clone().context("Member Email")?,
            from_email: vec!["noreply@dinko.space".to_string()],
            reply_to_email: "noreply@dinko.space".to_string(),
            body: String::from("{{SUBMISSION_LINK}}"),
            attachments: Vec::new(),
        }),
    )
    .await?;

    create_cms_collection(
        website_id.into(),
        CmsCreate {
            id: CollectionName {
                id: String::from("bookings"),
                ns: Some(String::from("@booking")),
            },
            name: String::from("Bookings Scheduled"),
            update: CmsUpdate::default(),
            columns: Some(vec![
                CmsCreateDataColumn {
                    id: String::from("bookDate"),
                    name: String::from("Book Date"),
                    type_of: SchematicFieldType::DateTime,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("bookID"),
                    name: String::from("Book ID"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("contactUuid"),
                    name: String::from("Contact UUID"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("duration"),
                    name: String::from("Duration"),
                    type_of: SchematicFieldType::Number,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("schemaDataUuid"),
                    name: String::from("Schema Data Uuid"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("service"),
                    name: String::from("Service"),
                    type_of: SchematicFieldType::Reference,
                    referenced_schema: Some(String::from("@booking:services")),
                },
                CmsCreateDataColumn {
                    id: String::from("staffMember"),
                    name: String::from("Staff Member"),
                    type_of: SchematicFieldType::Reference,
                    referenced_schema: Some(String::from("@booking:staff")),
                },
            ]),
            data: None,
            is_single: true,
        },
    )
    .await?;

    let services_cms = create_cms_collection(
        website_id.into(),
        CmsCreate {
            id: CollectionName {
                id: String::from("services"),
                ns: Some(String::from("@booking")),
            },
            name: String::from("Bookings Services"),
            update: CmsUpdate::default(),
            columns: Some(vec![
                CmsCreateDataColumn {
                    id: String::from("description"),
                    name: String::from("Description"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("image"),
                    name: String::from("Image"),
                    type_of: SchematicFieldType::Image,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("maxParticipants"),
                    name: String::from("Max Participants"),
                    type_of: SchematicFieldType::Number,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("NAME"),
                    name: String::from("Name"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("paymentType"),
                    name: String::from("Payment Type"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("priceAmount"),
                    name: String::from("Price Amount"),
                    type_of: SchematicFieldType::Number,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("type"),
                    name: String::from("Type"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("formId"),
                    name: String::from("Form"),
                    type_of: SchematicFieldType::Text,
                    // TODO: Somehow reference Forms here.
                    referenced_schema: None,
                },
            ]),
            data: Some(HashMap::from([
                (String::from("name"), vec!["Haircut".into()]),
                (String::from("paymentType"), vec!["in_person".into()]),
                (String::from("type"), vec!["appointment".into()]),
                (String::from("maxParticipants"), vec![1.into()]),
                (String::from("priceAmount"), vec![20.into()]),
                (String::from("formId"), vec![form.id.to_string().into()]),
            ])),
            is_single: true,
        },
    )
    .await?;

    let staff_cms = create_cms_collection(
        website_id.into(),
        CmsCreate {
            id: CollectionName {
                id: String::from("staff"),
                ns: Some(String::from("@booking")),
            },
            name: String::from("Bookings Staff"),
            update: CmsUpdate::default(),
            columns: Some(vec![
                CmsCreateDataColumn {
                    id: String::from("staffImage"),
                    name: String::from("Staff Image"),
                    type_of: SchematicFieldType::Image,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("staffName"),
                    name: String::from("Staff Name"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
            ]),
            data: Some(HashMap::from([(
                String::from("staffName"),
                vec!["Staff Member #1".into()],
            )])),
            is_single: true,
        },
    )
    .await?;

    let staff_ids = staff_cms.data_ids.context("Staff Ids")?;
    let service_ids = services_cms.data_ids.context("Uploaded Service Ids")?;

    let schedule_cms = create_cms_collection(
        website_id.into(),
        CmsCreate {
            id: CollectionName {
                id: String::from("schedule"),
                ns: Some(String::from("@booking")),
            },
            name: String::from("Bookings Schedule"),
            update: CmsUpdate::default(),
            columns: Some(vec![
                CmsCreateDataColumn {
                    id: String::from("break"),
                    name: String::from("Break"),
                    type_of: SchematicFieldType::Number,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("duration"),
                    name: String::from("Duration"),
                    type_of: SchematicFieldType::Number,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("repeats"),
                    name: String::from("Repeats"),
                    type_of: SchematicFieldType::Number,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("service"),
                    name: String::from("Service"),
                    type_of: SchematicFieldType::Reference,
                    referenced_schema: Some(String::from("@booking:services")),
                },
                CmsCreateDataColumn {
                    id: String::from("serviceSchedule"),
                    name: String::from("Service Schedule"),
                    type_of: SchematicFieldType::Object,
                    referenced_schema: None,
                },
            ]),
            data: Some(HashMap::from([
                (String::from("break"), vec![15.into()]),
                (String::from("duration"), vec![45.into()]),
                (String::from("repeats"), vec![1.into()]),
                (
                    String::from("service"),
                    vec![service_ids[0].to_string().into()],
                ),
                (
                    String::from("serviceSchedule"),
                    vec![SimpleValue::ObjectUnknown(serde_json::json!({
                        "fri": [], "mon": [], "sat": [], "sun": [], "thu": [], "tue": [], "wed": []
                    }))],
                ),
            ])),
            is_single: true,
        },
    )
    .await?;

    let schedule_ids = schedule_cms.data_ids.context("Schedule Ids")?;

    const DAYS: [&str; 5] = ["MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY"];

    create_cms_collection(
        website_id.into(),
        CmsCreate {
            id: CollectionName {
                id: String::from("staffSchedule"),
                ns: Some(String::from("@booking")),
            },
            name: String::from("Bookings Staff Schedule"),
            update: CmsUpdate::default(),
            columns: Some(vec![
                CmsCreateDataColumn {
                    id: String::from("name"),
                    name: String::from("Name"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("start"),
                    name: String::from("Start"),
                    type_of: SchematicFieldType::Time,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("end"),
                    name: String::from("End"),
                    type_of: SchematicFieldType::Time,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("recurrenceRule"),
                    name: String::from("Recurrence Rule"),
                    type_of: SchematicFieldType::Object,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("recurrenceType"),
                    name: String::from("Recurrence Type"),
                    type_of: SchematicFieldType::Tags,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("schedule"),
                    name: String::from("Schedule"),
                    type_of: SchematicFieldType::Reference,
                    referenced_schema: Some(String::from("@booking:schedule")),
                },
                CmsCreateDataColumn {
                    id: String::from("staff"),
                    name: String::from("Staff"),
                    type_of: SchematicFieldType::Reference,
                    referenced_schema: Some(String::from("@booking:staff")),
                },
                CmsCreateDataColumn {
                    id: String::from("startDay"),
                    name: String::from("Start Day"),
                    type_of: SchematicFieldType::Date,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("timeZone"),
                    name: String::from("Time Zone"),
                    type_of: SchematicFieldType::Text,
                    referenced_schema: None,
                },
                CmsCreateDataColumn {
                    id: String::from("type"),
                    name: String::from("Type"),
                    type_of: SchematicFieldType::Tags,
                    referenced_schema: None,
                },
            ]),
            data: Some(HashMap::from([
                (
                    String::from("name"),
                    (0..DAYS.len())
                        .map(|_| "business".into())
                        .collect::<Vec<_>>(),
                ),
                (
                    String::from("timeZone"),
                    (0..DAYS.len())
                        .map(|_| "America/Los_Angeles".into())
                        .collect::<Vec<_>>(),
                ),
                (
                    String::from("startDay"),
                    (0..DAYS.len())
                        .map(|i| {
                            time::Date::parse("2024-12-02", &date_format)
                                .unwrap()
                                .replace_day(2 + i as u8)
                                .unwrap()
                                .into()
                        })
                        .collect::<Vec<_>>(),
                ),
                (
                    String::from("start"),
                    (0..DAYS.len())
                        .map(|_| {
                            time::Time::parse("10:00:00.0", &time_format)
                                .unwrap()
                                .into()
                        })
                        .collect::<Vec<_>>(),
                ),
                (
                    String::from("end"),
                    (0..DAYS.len())
                        .map(|_| {
                            time::Time::parse("18:00:00.0", &time_format)
                                .unwrap()
                                .into()
                        })
                        .collect::<Vec<_>>(),
                ),
                (
                    String::from("schedule"),
                    (0..DAYS.len())
                        .map(|_| schedule_ids[0].to_string().into())
                        .collect::<Vec<_>>(),
                ),
                (
                    String::from("staff"),
                    (0..DAYS.len())
                        .map(|_| staff_ids[0].to_string().into())
                        .collect::<Vec<_>>(),
                ),
                (
                    String::from("recurrenceType"),
                    (0..DAYS.len())
                        .map(|_| "INSTANCE".into())
                        .collect::<Vec<_>>(),
                ),
                (
                    String::from("type"),
                    (0..DAYS.len())
                        .map(|_| "WORKING_HOURS".into())
                        .collect::<Vec<_>>(),
                ),
                (
                    String::from("recurrenceRule"),
                    DAYS.iter()
                        .map(|day| {
                            serde_json::from_str::<serde_json::Value>(&format!(
                                r#"{{ "days": [ "{day}" ], "frequency": "WEEKLY", "interval": 1 }}"#
                            ))
                            .unwrap()
                            .into()
                        })
                        .collect::<Vec<_>>(),
                ),
            ])),
            is_single: false,
        },
    )
    .await?;

    Ok(Json(WrappingResponse::okay(InstallResponse::Complete)))
}
