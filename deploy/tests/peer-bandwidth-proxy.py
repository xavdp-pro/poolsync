#!/usr/bin/env python3
"""Disposable per-peer TCP bandwidth fixture; payload bytes stay opaque.

Every test desktop owns its proxy. No central relay/hub or production network
rule is introduced. The proxy terminates with its isolated fixture process.
"""
import argparse
import asyncio


async def serve(listen_port, backend_port, bytes_per_second):
    async def pipe(reader, writer):
        next_write = asyncio.get_running_loop().time()
        while data := await reader.read(16384):
            now = asyncio.get_running_loop().time()
            next_write = max(next_write, now) + len(data) / bytes_per_second
            await asyncio.sleep(max(0, next_write - now))
            writer.write(data)
            await writer.drain()

    async def client(reader, writer):
        backend = None
        tasks = []
        try:
            remote, backend = await asyncio.open_connection('127.0.0.1', backend_port)
            tasks = [asyncio.create_task(pipe(reader, backend)),
                     asyncio.create_task(pipe(remote, writer))]
            done, _ = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
            for task in done:
                task.result()
        except (ConnectionError, OSError):
            pass
        finally:
            for task in tasks:
                task.cancel()
            if tasks:
                await asyncio.gather(*tasks, return_exceptions=True)
            for connection in (writer, backend):
                if connection:
                    connection.close()
                    try:
                        await connection.wait_closed()
                    except (ConnectionError, OSError):
                        pass

    server = await asyncio.start_server(client, '0.0.0.0', listen_port)
    async with server:
        await server.serve_forever()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--listen-port', type=int, required=True)
    parser.add_argument('--backend-port', type=int, required=True)
    parser.add_argument('--rate-mbit', type=float, required=True)
    args = parser.parse_args()
    assert 1024 <= args.listen_port <= 65535 and 1024 <= args.backend_port <= 65535
    assert args.listen_port != args.backend_port and 0 < args.rate_mbit <= 1000
    asyncio.run(serve(args.listen_port, args.backend_port, args.rate_mbit * 1_000_000 / 8))
