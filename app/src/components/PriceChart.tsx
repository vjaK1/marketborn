import * as echarts from 'echarts';
import { useEffect, useRef } from 'react';
import { formatMoney, formatTickLabel, goodLabel } from '../format';
import type { PriceHistory } from '../types';

/**
 * Daily average execution price per good. Design per the dataviz method:
 * categorical slots in fixed order (validated on this surface), 2px lines,
 * recessive grid, axis crosshair + tooltip, legend + direct end labels,
 * text in ink tokens (never series colors).
 */

// Categorical slots 1-6 of the dark-mode reference palette, assigned in
// Good::ALL order, never cycled. Validated as a set on surface #1e2128
// (all >= 3:1; worst adjacent CVD pair sits in the floor band, which is
// legal because every series carries a direct end label plus the legend).
const SERIES_COLORS: Record<string, string> = {
  wheat: '#3987e5',
  flour: '#199e70',
  food: '#c98500',
  'iron ore': '#008300',
  steel: '#9085e9',
  tools: '#e66767',
};

const INK_SECONDARY = '#a7adba';
const INK_MUTED = '#8b91a0';
const GRIDLINE = '#262b34';
const AXIS = '#343945';

export function PriceChart({ history }: { history: PriceHistory }) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const chartRef = useRef<echarts.ECharts | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const chart = echarts.init(host);
    chartRef.current = chart;
    const observer = new ResizeObserver(() => chart.resize());
    observer.observe(host);
    return () => {
      observer.disconnect();
      chart.dispose();
      chartRef.current = null;
    };
  }, []);

  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;
    chart.setOption(
      {
        animation: false,
        backgroundColor: 'transparent',
        legend: {
          top: 4,
          left: 10,
          icon: 'roundRect',
          itemWidth: 12,
          itemHeight: 4,
          textStyle: { color: INK_SECONDARY, fontSize: 11 },
        },
        grid: { left: 52, right: 74, top: 30, bottom: 26 },
        tooltip: {
          trigger: 'axis',
          axisPointer: { type: 'line', lineStyle: { color: AXIS } },
          backgroundColor: '#252932',
          borderColor: AXIS,
          textStyle: { color: '#d6d9e0', fontSize: 12 },
          valueFormatter: (value: unknown) =>
            typeof value === 'number' ? formatMoney(value) : '—',
        },
        xAxis: {
          type: 'category',
          boundaryGap: false,
          data: history.ticks.map(String),
          axisLine: { lineStyle: { color: AXIS } },
          axisTick: { show: false },
          axisLabel: {
            color: INK_MUTED,
            fontSize: 10.5,
            interval: Math.max(0, Math.floor(history.ticks.length / 6) - 1),
            formatter: (value: string) => formatTickLabel(Number(value)),
          },
        },
        yAxis: {
          type: 'value',
          scale: true,
          axisLabel: {
            color: INK_MUTED,
            fontSize: 10.5,
            formatter: (value: number) => formatMoney(value),
          },
          splitLine: { lineStyle: { color: GRIDLINE } },
        },
        series: history.series.map((s) => ({
          name: goodLabel(s.good),
          type: 'line' as const,
          data: s.points,
          connectNulls: true,
          showSymbol: false,
          lineStyle: { width: 2 },
          color: SERIES_COLORS[s.good] ?? INK_MUTED,
          emphasis: { focus: 'series' as const },
          endLabel: {
            show: true,
            formatter: '{a}',
            color: INK_SECONDARY,
            fontSize: 10.5,
            distance: 8,
          },
          labelLayout: { moveOverlap: 'shiftY' as const },
        })),
      },
      { notMerge: false, replaceMerge: ['series'] },
    );
  }, [history]);

  return <div ref={hostRef} className="chart-host" />;
}
