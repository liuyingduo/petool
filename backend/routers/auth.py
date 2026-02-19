from fastapi import APIRouter, HTTPException, status
from bson import ObjectId
from datetime import datetime
from database import get_database
from schemas import RegisterRequest, LoginRequest, TokenResponse
from auth_utils import verify_password, hash_password, create_access_token

router = APIRouter(prefix="/auth", tags=["认证"])

# 新用户注册赠送 token 数
NEW_USER_WELCOME_TOKENS = 50_000


@router.post("/register", response_model=TokenResponse, summary="用户注册")
async def register(body: RegisterRequest):
    db = get_database()

    # 检查邮箱/用户名是否已存在
    if await db.users.find_one({"email": body.email}):
        raise HTTPException(status_code=400, detail="该邮箱已注册")
    if await db.users.find_one({"username": body.username}):
        raise HTTPException(status_code=400, detail="该用户名已被使用")

    now = datetime.utcnow()
    doc = {
        "username": body.username,
        "email": body.email,
        "password_hash": hash_password(body.password),
        "token_balance": NEW_USER_WELCOME_TOKENS,
        "token_total_used": 0,
        "membership_level": "free",
        "membership_expire_at": None,
        "avatar": None,
        "created_at": now,
        "updated_at": now,
    }
    result = await db.users.insert_one(doc)
    user_id = str(result.inserted_id)

    access_token = create_access_token(user_id, body.username)
    return TokenResponse(
        access_token=access_token,
        user_id=user_id,
        username=body.username,
        email=body.email,
    )


@router.post("/login", response_model=TokenResponse, summary="用户登录")
async def login(body: LoginRequest):
    db = get_database()
    user = await db.users.find_one({"email": body.email})
    if not user or not verify_password(body.password, user["password_hash"]):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="邮箱或密码错误",
        )

    user_id = str(user["_id"])
    access_token = create_access_token(user_id, user["username"])
    return TokenResponse(
        access_token=access_token,
        user_id=user_id,
        username=user["username"],
        email=user["email"],
    )
