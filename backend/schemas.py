from datetime import datetime
from typing import Optional
from pydantic import BaseModel, EmailStr, Field
from bson import ObjectId


# ─── 用户集合 Schema ───────────────────────────────────────────────────────────

class UserInDB(BaseModel):
    """MongoDB users 集合文档结构"""
    id: Optional[str] = Field(None, alias="_id")
    username: str
    email: str
    password_hash: str
    token_balance: int = 0               # 剩余 token 数
    token_total_used: int = 0            # 累计消耗 token
    membership_level: str = "free"       # free | pro | enterprise
    membership_expire_at: Optional[datetime] = None
    avatar: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        populate_by_name = True
        arbitrary_types_allowed = True


# ─── 用量记录集合 Schema ───────────────────────────────────────────────────────

class UsageRecordInDB(BaseModel):
    """MongoDB usage_records 集合文档结构"""
    id: Optional[str] = Field(None, alias="_id")
    user_id: str
    model: str
    task_type: str = "对话"              # 对话 / 文档生成 / 图片生成 等
    prompt_tokens: int = 0
    completion_tokens: int = 0
    total_tokens: int = 0
    cost_tokens: int = 0                # 实际扣减（不同模型系数不同）
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        populate_by_name = True
        arbitrary_types_allowed = True


# ─── 订单集合 Schema ───────────────────────────────────────────────────────────

class OrderInDB(BaseModel):
    """MongoDB orders 集合文档结构"""
    id: Optional[str] = Field(None, alias="_id")
    user_id: str
    plan_id: str                         # monthly | yearly | pack-100w 等
    title: str
    amount: float                        # 金额（元）
    token_grant: int = 0                 # 充值 token 数
    days_grant: int = 0                  # 延长天数（会员）
    payment_method: str = "wechat"       # wechat | alipay
    out_trade_no: str                    # 商户订单号（唯一）
    transaction_id: Optional[str] = None # 第三方支付流水号
    status: str = "pending"             # pending | paid | refunded | cancelled
    paid_at: Optional[datetime] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        populate_by_name = True
        arbitrary_types_allowed = True


# ─── API 请求/响应 Pydantic Schema ────────────────────────────────────────────

class RegisterRequest(BaseModel):
    username: str = Field(..., min_length=2, max_length=30)
    email: EmailStr
    password: str = Field(..., min_length=6)


class LoginRequest(BaseModel):
    email: EmailStr
    password: str


class TokenResponse(BaseModel):
    access_token: str
    token_type: str = "bearer"
    user_id: str
    username: str
    email: str


class UserProfile(BaseModel):
    user_id: str
    username: str
    email: str
    avatar: Optional[str]
    membership_level: str
    membership_expire_at: Optional[datetime]
    days_left: int
    token_balance: int
    token_total_used: int
    token_usage_percent: float


class QuotaDashboard(BaseModel):
    total_balance: int
    consumed_today: int
    trend: list  # [{date, value}]


class UsageRecordOut(BaseModel):
    id: str
    created_at: str
    task_type: str
    model: str
    prompt_tokens: int
    completion_tokens: int
    total_tokens: int
    cost_tokens: int


class UsagePage(BaseModel):
    records: list[UsageRecordOut]
    total: int
    page: int
    page_size: int


class OrderOut(BaseModel):
    id: str
    title: str
    amount: float
    plan_id: str
    payment_method: str
    status: str
    created_at: str


class CreateOrderRequest(BaseModel):
    plan_id: str
    payment_method: str = "wechat"   # wechat | alipay


class CreateOrderResponse(BaseModel):
    out_trade_no: str
    payment_method: str
    # 微信支付返回 code_url (扫码链接)，支付宝返回 pay_url（跳转链接）
    code_url: Optional[str] = None
    pay_url: Optional[str] = None
