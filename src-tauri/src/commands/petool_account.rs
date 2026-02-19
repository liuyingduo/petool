use crate::utils::{load_config, save_config};
use crate::models::config::Config;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub user_id: String,
    pub username: String,
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub avatar: Option<String>,
    pub membership_level: String,
    pub membership_expire_at: Option<String>,
    pub days_left: i64,
    pub token_balance: i64,
    pub token_total_used: i64,
    pub token_usage_percent: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuotaDashboard {
    pub total_balance: i64,
    pub consumed_today: i64,
    pub trend: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsagePage {
    pub records: Vec<serde_json::Value>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

fn get_petool_base(config: &Config) -> String {
    config
        .petool_api_base
        .clone()
        .unwrap_or_else(|| "http://localhost:8000".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn get_petool_token(config: &Config) -> Result<String, String> {
    config
        .petool_token
        .clone()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "未登录，请先登录账户".to_string())
}

/// 用户注册
#[tauri::command]
pub async fn petool_register(
    username: String,
    email: String,
    password: String,
) -> Result<LoginResponse, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let base = get_petool_base(&config);

    let client = Client::new();
    let resp = client
        .post(format!("{}/auth/register", base))
        .json(&serde_json::json!({ "username": username, "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    if !resp.status().is_success() {
        let err: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(err["detail"].as_str().unwrap_or("注册失败").to_string());
    }

    let login_resp: LoginResponse = resp.json().await.map_err(|e| e.to_string())?;

    // 保存 token 到本地配置
    let mut cfg = load_config::<Config>().unwrap_or_default();
    cfg.petool_token = Some(login_resp.access_token.clone());
    save_config(&cfg).map_err(|e| e.to_string())?;

    Ok(login_resp)
}

/// 用户登录
#[tauri::command]
pub async fn petool_login(email: String, password: String) -> Result<LoginResponse, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let base = get_petool_base(&config);

    let client = Client::new();
    let resp = client
        .post(format!("{}/auth/login", base))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    if !resp.status().is_success() {
        let err: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(err["detail"].as_str().unwrap_or("登录失败").to_string());
    }

    let login_resp: LoginResponse = resp.json().await.map_err(|e| e.to_string())?;

    // 保存 token 到本地配置
    let mut cfg = load_config::<Config>().unwrap_or_default();
    cfg.petool_token = Some(login_resp.access_token.clone());
    save_config(&cfg).map_err(|e| e.to_string())?;

    Ok(login_resp)
}

/// 退出登录（清除本地 Token）
#[tauri::command]
pub async fn petool_logout() -> Result<(), String> {
    let mut config = load_config::<Config>().map_err(|e| e.to_string())?;
    config.petool_token = None;
    save_config(&config).map_err(|e| e.to_string())?;
    Ok(())
}

/// 检查当前登录状态
#[tauri::command]
pub async fn petool_is_logged_in() -> Result<bool, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    Ok(config.petool_token.as_deref().map(|t| !t.is_empty()).unwrap_or(false))
}

/// 获取个人资料
#[tauri::command]
pub async fn petool_get_profile() -> Result<UserProfile, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let base = get_petool_base(&config);
    let token = get_petool_token(&config)?;

    let client = Client::new();
    let resp = client
        .get(format!("{}/account/profile", base))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    if resp.status().as_u16() == 401 {
        return Err("登录已过期，请重新登录".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("获取资料失败: {}", resp.status()));
    }

    resp.json::<UserProfile>().await.map_err(|e| e.to_string())
}

/// 获取额度仪表盘
#[tauri::command]
pub async fn petool_get_quota() -> Result<QuotaDashboard, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let base = get_petool_base(&config);
    let token = get_petool_token(&config)?;

    let client = Client::new();
    let resp = client
        .get(format!("{}/account/quota", base))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("获取额度失败: {}", resp.status()));
    }

    resp.json::<QuotaDashboard>().await.map_err(|e| e.to_string())
}

/// 获取消费明细（分页）
#[tauri::command]
pub async fn petool_get_usage(page: Option<i64>, page_size: Option<i64>) -> Result<UsagePage, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let base = get_petool_base(&config);
    let token = get_petool_token(&config)?;

    let p = page.unwrap_or(1);
    let ps = page_size.unwrap_or(10);

    let client = Client::new();
    let resp = client
        .get(format!("{}/account/usage?page={}&page_size={}", base, p, ps))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("获取明细失败: {}", resp.status()));
    }

    resp.json::<UsagePage>().await.map_err(|e| e.to_string())
}

/// 获取订单列表
#[tauri::command]
pub async fn petool_get_orders() -> Result<Vec<serde_json::Value>, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let base = get_petool_base(&config);
    let token = get_petool_token(&config)?;

    let client = Client::new();
    let resp = client
        .get(format!("{}/account/orders", base))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("获取订单失败: {}", resp.status()));
    }

    resp.json::<Vec<serde_json::Value>>().await.map_err(|e| e.to_string())
}

/// 创建支付订单（微信 / 支付宝）
#[tauri::command]
pub async fn petool_create_order(
    plan_id: String,
    payment_method: String,
) -> Result<serde_json::Value, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let base = get_petool_base(&config);
    let token = get_petool_token(&config)?;

    let endpoint = if payment_method == "alipay" {
        "payment/alipay/create"
    } else {
        "payment/wechat/create"
    };

    let client = Client::new();
    let resp = client
        .post(format!("{}/{}", base, endpoint))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "plan_id": plan_id, "payment_method": payment_method }))
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    if !resp.status().is_success() {
        let err: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(err["detail"].as_str().unwrap_or("下单失败").to_string());
    }

    resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())
}

/// 查询订单支付状态（前端轮询用）
#[tauri::command]
pub async fn petool_query_order(out_trade_no: String) -> Result<serde_json::Value, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let base = get_petool_base(&config);
    let token = get_petool_token(&config)?;

    let client = Client::new();
    let resp = client
        .get(format!("{}/payment/order/{}", base, out_trade_no))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("查询失败: {}", resp.status()));
    }

    resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())
}
