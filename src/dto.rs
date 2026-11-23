use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------- Auth ----------
#[derive(Deserialize)]
pub struct RegisterRequest { pub login: String, pub password: String, pub name: String }

#[derive(Deserialize)]
pub struct LoginRequest { pub login: String, pub password: String }

#[derive(Serialize)]
pub struct UserDto {
    pub id: i64, pub login: String, pub name: String,
    pub role: String, pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct LoginResponse { pub token: String, pub expires_at: DateTime<Utc> }

// ---------- Surveys ----------
#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType { SingleChoice, MultipleChoice, Text, Number, Boolean }

#[derive(Deserialize)]
pub struct NewOption { pub text: String, pub position: i32 }

#[derive(Deserialize)]
pub struct NewQuestion {
    pub text: String,
    #[serde(rename = "type")] pub qtype: QuestionType,
    pub is_required: bool,
    pub position: i32,
    #[serde(default)] pub options: Vec<NewOption>,
}

#[derive(Deserialize)]
pub struct CreateSurveyRequest {
    pub title: String,
    pub description: Option<String>,
    #[serde(default)] pub is_anonymous: bool,
    #[serde(default)] pub only_one_response: bool,
    pub questions: Vec<NewQuestion>,
}

#[derive(Deserialize)]
pub struct UpdateSurveyRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub is_anonymous: Option<bool>,
    pub only_one_response: Option<bool>,
}

#[derive(Serialize)]
pub struct OptionDto { pub id: i64, pub text: String, pub position: i32 }

#[derive(Serialize)]
pub struct QuestionDto {
    pub id: i64, pub text: String, pub r#type: QuestionType,
    pub is_required: bool, pub position: i32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OptionDto>,
}

#[derive(Serialize)]
pub struct SurveyDto {
    pub id: i64, pub link: String, pub title: String,
    pub description: Option<String>,
    pub is_anonymous: bool, pub only_one_response: bool,
    pub created_at: DateTime<Utc>,
    pub questions: Vec<QuestionDto>,
}

#[derive(Serialize)]
pub struct SurveySummaryDto {
    pub id: i64, pub title: String, pub link: String,
    pub created_at: DateTime<Utc>,
    pub response_count: i64,
}

#[derive(Serialize)]
pub struct SurveyListResponse { pub items: Vec<SurveySummaryDto> }

// ---------- Responses ----------
#[derive(Deserialize)]
pub struct NewAnswer {
    pub question_id: i64,
    #[serde(default)] pub option_id:     Option<i64>,
    #[serde(default)] pub text_value:    Option<String>,
    #[serde(default)] pub numeric_value: Option<f64>,
    #[serde(default)] pub bool_value:    Option<bool>,
}

#[derive(Deserialize)]
pub struct SubmitResponseRequest { pub answers: Vec<NewAnswer> }

#[derive(Serialize)]
pub struct SubmitResponseReply { pub response_id: i64, pub created_at: DateTime<Utc> }

#[derive(Serialize)]
pub struct ResponseListItem {
    pub response_id: i64,
    pub created_at:  DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserBriefDto>,
}

#[derive(Serialize)]
pub struct UserBriefDto { pub id: i64, pub login: String, pub name: String }

#[derive(Serialize)]
pub struct ResponseListResponse { pub items: Vec<ResponseListItem> }

#[derive(Deserialize)]
pub struct PageParams {
    pub limit:  Option<i64>,
    pub offset: Option<i64>,
}
impl PageParams {
    pub fn limit_or(&self, def: i64) -> i64 {
        self.limit.unwrap_or(def).clamp(1, 200)
    }
    pub fn offset_or0(&self) -> i64 { self.offset.unwrap_or(0).max(0) }
}