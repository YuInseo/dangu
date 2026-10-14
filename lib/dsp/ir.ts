/**
 * 컨볼루션용 임펄스 응답 파일. DATA_DIR/ir 에 둔다.
 */
import { randomBytes } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { parseBuffer } from 'music-metadata';
import { config } from '../config';

export interface IrInfo {
  id: string;
  name: string;
  ext: string;
  channels: number;
  sampleRate: number;
  duration: number;
  size: number;
  createdAt: number;
}

const dir = () => path.join(config.dataDir, 'ir');
const indexFile = () => path.join(dir(), 'index.json');
const ID = /^[a-z0-9]{6,40}$/;
const EXT = new Set(['wav', 'flac', 'aif', 'aiff']);
export const IR_MAX_BYTES = 50 * 1024 * 1024;

async function readIndex(): Promise<IrInfo[]> {
  try {
    return JSON.parse(await fs.readFile(indexFile(), 'utf8')) as IrInfo[];
  } catch {
    return [];
  }
}

async function writeIndex(list: IrInfo[]) {
  await fs.mkdir(dir(), { recursive: true });
  const tmp = `${indexFile()}.${process.pid}.tmp`;
  await fs.writeFile(tmp, JSON.stringify(list, null, 1));
  await fs.rename(tmp, indexFile());
}

export async function listIrs(): Promise<IrInfo[]> {
  return (await readIndex()).sort((a, b) => b.createdAt - a.createdAt);
}

export async function getIr(id: string): Promise<(IrInfo & { file: string }) | null> {
  if (!ID.test(id)) return null;
  const info = (await readIndex()).find((x) => x.id === id);
  if (!info) return null;
  const file = path.join(dir(), `${id}.${info.ext}`);
  try {
    await fs.access(file);
    return { ...info, file };
  } catch {
    return null;
  }
}

export class IrError extends Error {}

export async function saveIr(name: string, data: Uint8Array): Promise<IrInfo> {
  const ext = path.extname(name).slice(1).toLowerCase();
  if (!EXT.has(ext)) throw new IrError('WAV, FLAC, AIFF 파일만 받습니다');
  if (data.byteLength > IR_MAX_BYTES) throw new IrError('50MB보다 큰 파일은 받지 않습니다');
  let meta;
  try {
    meta = await parseBuffer(
      data,
      { mimeType: ext === 'flac' ? 'audio/flac' : ext === 'wav' ? 'audio/wav' : 'audio/aiff', size: data.byteLength },
      { duration: true },
    );
  } catch {
    throw new IrError('오디오 파일로 읽을 수 없습니다');
  }
  const channels = meta.format.numberOfChannels ?? 0;
  const sampleRate = meta.format.sampleRate ?? 0;
  const duration = meta.format.duration ?? 0;
  if (channels < 1 || channels > 2) throw new IrError('모노나 스테레오 임펄스 응답만 쓸 수 있습니다');
  if (!sampleRate) throw new IrError('샘플레이트를 알 수 없습니다');
  if (duration > 30) throw new IrError('30초보다 긴 임펄스 응답은 쓸 수 없습니다');

  const id = randomBytes(9).toString('hex');
  await fs.mkdir(dir(), { recursive: true });
  await fs.writeFile(path.join(dir(), `${id}.${ext}`), data);
  const info: IrInfo = {
    id,
    name: path.basename(name).slice(0, 120),
    ext,
    channels,
    sampleRate,
    duration: Math.round(duration * 1000) / 1000,
    size: data.byteLength,
    createdAt: Date.now(),
  };
  await writeIndex([...(await readIndex()), info]);
  return info;
}

export async function deleteIr(id: string): Promise<boolean> {
  const info = await getIr(id);
  if (!info) return false;
  await fs.rm(info.file, { force: true });
  await writeIndex((await readIndex()).filter((x) => x.id !== id));
  return true;
}
