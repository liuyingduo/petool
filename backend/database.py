from motor.motor_asyncio import AsyncIOMotorClient
from pymongo import ASCENDING, DESCENDING
import os
from dotenv import load_dotenv

load_dotenv()

MONGODB_URL = os.getenv("MONGODB_URL", "mongodb://localhost:27017")
MONGODB_DB = os.getenv("MONGODB_DB", "petool")

client: AsyncIOMotorClient | None = None


def get_database():
    return client[MONGODB_DB]


async def connect_db():
    global client
    client = AsyncIOMotorClient(MONGODB_URL)
    db = get_database()

    # 创建索引
    await db.users.create_index([("email", ASCENDING)], unique=True)
    await db.users.create_index([("username", ASCENDING)], unique=True)
    await db.usage_records.create_index([("user_id", ASCENDING), ("created_at", DESCENDING)])
    await db.orders.create_index([("user_id", ASCENDING), ("created_at", DESCENDING)])
    await db.orders.create_index([("out_trade_no", ASCENDING)], unique=True, sparse=True)

    print(f"✅ MongoDB 已连接: {MONGODB_URL}/{MONGODB_DB}")


async def close_db():
    global client
    if client:
        client.close()
        print("MongoDB 连接已关闭")
