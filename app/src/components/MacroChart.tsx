import * as echarts from 'echarts';
import { useEffect, useRef } from 'react';
import { formatMoney, formatTickLabel } from '../format';
import type { MacroHistory } from '../types';

/**
 * The society & fiscal history (BRIEF: "historical charts"): employment
 * and hunger as counts on the left axis, treasury and sovereign debt as
 * money on the right — the macro story of the town over the same window
 * the price chart shows. Same dataviz method as PriceChart: fixed slots,
 * 2px lines, recessive grid, direct end labels.
 */

const INK_SECONDARY = '#a7adba';
const INK_MUTED = '#8b91a0';
const GRIDLINE = '#262b34';
const AXIS = '#343945';

const COUNT_SERIES = 2; // employed, hungry — the rest are money

export function MacroChart({ history }: { history: MacroHistory }) {
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
    const line = (
      name: string,
      data: number[],
      color: string,
      yAxisIndex: number,
    ) => ({
      name,
      type: 'line' as const,
      data,
      yAxisIndex,
      showSymbol: false,
      lineStyle: { width: 2 },
      color,
      emphasis: { focus: 'series' as const },
      endLabel: {
        show: true,
        formatter: '{a}',
        color: INK_SECONDARY,
        fontSize: 10.5,
        distance: 8,
      },
      labelLayout: { moveOverlap: 'shiftY' as const },
    });
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
        grid: { left: 40, right: 92, top: 30, bottom: 26 },
        tooltip: {
          trigger: 'axis',
          axisPointer: { type: 'line', lineStyle: { color: AXIS } },
          backgroundColor: '#252932',
          borderColor: AXIS,
          textStyle: { color: '#d6d9e0', fontSize: 12 },
          formatter: (params: unknown) => {
            const rows = params as {
              seriesName: string;
              seriesIndex: number;
              value: number;
              axisValueLabel: string;
              marker: string;
            }[];
            if (!Array.isArray(rows) || rows.length === 0) return '';
            const head = formatTickLabel(Number(rows[0]?.axisValueLabel));
            const body = rows
              .map((r) => {
                const v =
                  r.seriesIndex < COUNT_SERIES
                    ? String(r.value)
                    : formatMoney(r.value);
                return `${r.marker} ${r.seriesName}: <b>${v}</b>`;
              })
              .join('<br/>');
            return `${head}<br/>${body}`;
          },
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
        yAxis: [
          {
            type: 'value',
            name: 'people',
            nameTextStyle: { color: INK_MUTED, fontSize: 10 },
            minInterval: 1,
            axisLabel: { color: INK_MUTED, fontSize: 10.5 },
            splitLine: { lineStyle: { color: GRIDLINE } },
          },
          {
            type: 'value',
            name: '$',
            nameTextStyle: { color: INK_MUTED, fontSize: 10 },
            scale: true,
            axisLabel: {
              color: INK_MUTED,
              fontSize: 10.5,
              formatter: (value: number) => formatMoney(value),
            },
            splitLine: { show: false },
          },
        ],
        series: [
          line('Employed', history.employed, '#3987e5', 0),
          line('Hungry', history.hungry, '#e66767', 0),
          line('Treasury', history.govt_cash_cents, '#199e70', 1),
          line('Govt debt', history.govt_debt_cents, '#c98500', 1),
        ],
      },
      { notMerge: false, replaceMerge: ['series'] },
    );
  }, [history]);

  return <div ref={hostRef} className="chart-host" />;
}
