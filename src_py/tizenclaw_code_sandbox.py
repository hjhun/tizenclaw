#!/usr/bin/env python3
import asyncio
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

async def run():
    server = await asyncio.start_unix_server(
        lambda r, w: None,
        path="\0tizenclaw_code_sandbox.sock"
    )
    logger.info("Python Code Sandbox listening..")
    async with server:
        await server.serve_forever()

if __name__ == "__main__":
    asyncio.run(run())
