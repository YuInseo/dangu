import { NextResponse } from 'next/server';
import { getCaps } from '@/lib/dsp/caps';

export async function GET() {
  return NextResponse.json(await getCaps());
}
