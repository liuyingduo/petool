"""
Petool 后端中转服务 - FastAPI 主入口
"""
from contextlib import asynccontextmanager
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from dotenv import load_dotenv

load_dotenv()

from database import connect_db, close_db
from routers import auth, account, proxy, payment, ocr


@asynccontextmanager
async def lifespan(app: FastAPI):
    await connect_db()
    yield
    await close_db()


app = FastAPI(
    title="Petool 后端服务",
    description="AI 请求中转 + 用户账户管理 + 支付接口",
    version="1.0.0",
    lifespan=lifespan,
)

# CORS 配置（Tauri 桌面应用使用 tauri://localhost 或 http://localhost）
app.add_middleware(
    CORSMiddleware,
    allow_origins=[
        "tauri://localhost",
        "https://tauri.localhost",
        "http://localhost",
        "http://localhost:1420",
        "http://localhost:5173",
        "*",  # 开发阶段放开，生产环境删除此行
    ],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# 注册路由
app.include_router(auth.router)
app.include_router(account.router)
app.include_router(proxy.router)
app.include_router(payment.router)
app.include_router(ocr.router)


@app.get("/", tags=["健康检测"])
async def root():
    return {"status": "ok", "service": "Petool Backend"}


@app.get("/health", tags=["健康检测"])
async def health():
    return {"status": "healthy"}
