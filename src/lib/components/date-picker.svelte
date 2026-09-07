<script lang="ts">
  // 日期选择器：Popover 触发按钮（日历图标 + 文本）+ 自绘月历。
  import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-svelte";
  import { Popover } from "bits-ui";
  import Button from "$lib/components/ui/button/button.svelte";

  let {
    value = "",
    onPick,
  }: { value?: string; onPick?: (v: string) => void } = $props();

  let open = $state(false);
  let viewYear = $state(0);
  let viewMonth = $state(0); // 0-11

  function todayStr(d = new Date()) {
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  }
  const today = $derived(todayStr());

  function initView() {
    if (value) {
      const [y, m] = value.split("-").map(Number);
      viewYear = y;
      viewMonth = m - 1;
    } else {
      const d = new Date();
      viewYear = d.getFullYear();
      viewMonth = d.getMonth();
    }
  }

  // 触发按钮文案：今天 →「今天」，否则 YYYY-MM-DD
  const label = $derived(value ? (value === today ? "今天" : value) : "今天");

  // 当月格子：前置空位（周一起始）+ 每日
  const days = $derived.by(() => {
    const year = viewYear;
    const month = viewMonth;
    const first = new Date(year, month, 1);
    const offset = (first.getDay() + 6) % 7; // 周一 = 0
    const count = new Date(year, month + 1, 0).getDate();
    const out: (string | null)[] = [];
    for (let i = 0; i < offset; i++) out.push(null);
    for (let d = 1; d <= count; d++) {
      out.push(`${year}-${String(month + 1).padStart(2, "0")}-${String(d).padStart(2, "0")}`);
    }
    return out;
  });

  const weekdays = ["一", "二", "三", "四", "五", "六", "日"];

  function prevMonth() {
    viewMonth--;
    if (viewMonth < 0) {
      viewMonth = 11;
      viewYear--;
    }
  }
  function nextMonth() {
    viewMonth++;
    if (viewMonth > 11) {
      viewMonth = 0;
      viewYear++;
    }
  }

  function pick(d: string) {
    onPick?.(d);
    open = false;
  }
</script>

<Popover.Root bind:open onOpenChange={(o) => { open = o; if (o) initView(); }}>
  <Popover.Trigger>
    <Button variant="outline" size="sm" type="button" onclick={(e) => { e.preventDefault(); }}>
      <CalendarDays size={15} />
      <span>{label}</span>
    </Button>
  </Popover.Trigger>
  <Popover.Content side="bottom" align="start" sideOffset={6} class="date-content">
    <div class="date-head">
      <button class="date-nav" type="button" onclick={prevMonth} aria-label="上个月">
        <ChevronLeft size={14} />
      </button>
      <span class="date-title">{viewYear} 年 {viewMonth + 1} 月</span>
      <button class="date-nav" type="button" onclick={nextMonth} aria-label="下个月">
        <ChevronRight size={14} />
      </button>
    </div>
    <div class="date-grid">
      {#each weekdays as wd (wd)}
        <span class="date-wd">{wd}</span>
      {/each}
      {#each days as d, i (i)}
        {#if d === null}
          <span class="date-empty"></span>
        {:else}
          <button
            type="button"
            class="date-day"
            class:selected={d === value}
            class:today={d === today}
            onclick={() => pick(d)}
          >
            {Number(d.slice(8))}
          </button>
        {/if}
      {/each}
    </div>
  </Popover.Content>
</Popover.Root>

<style>
  :global(.date-content) {
    width: 264px;
    padding: 12px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: var(--shadow-md);
    z-index: 60;
  }
  .date-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .date-title {
    font-family: var(--font-display);
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-hi);
  }
  .date-nav {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .date-nav:hover {
    background: rgba(66, 184, 131, 0.1);
    color: var(--primary-deep);
  }
  .date-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 2px;
  }
  .date-wd {
    font-size: 10px;
    color: var(--muted);
    text-align: center;
    padding: 3px 0;
  }
  .date-empty {
    height: 26px;
  }
  .date-day {
    height: 26px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font-size: 11.5px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .date-day:hover {
    background: rgba(66, 184, 131, 0.1);
  }
  .date-day.today {
    box-shadow: inset 0 0 0 1px var(--primary);
    color: var(--primary-deep);
    font-weight: 600;
  }
  .date-day.selected {
    background: var(--primary);
    color: #fff;
    font-weight: 600;
  }
</style>
