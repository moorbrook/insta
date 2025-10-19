#!/usr/bin/env -S uv run --quiet --script
# /// script
# dependencies = [
#   "pandas",
#   "matplotlib",
#   "seaborn",
#   "numpy",
#   "plotly",
# ]
# ///

"""
Instapaper Bookmarks Analysis Script
Analyzes bookmark patterns, creates visualizations, and suggests categorization
"""

import sqlite3
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np
from datetime import datetime, timedelta
from collections import Counter
import plotly.graph_objects as go
from urllib.parse import urlparse

# Set style
sns.set_style("whitegrid")
plt.rcParams['figure.figsize'] = (15, 8)

# Connect to database
conn = sqlite3.connect('./articles/index.db')

# Load data
df = pd.read_sql_query("SELECT * FROM articles", conn)
conn.close()

# Convert timestamp to datetime (UTC) and then to local timezone
df['datetime'] = pd.to_datetime(df['timestamp'], unit='s', utc=True)
df['datetime'] = df['datetime'].dt.tz_convert('America/Los_Angeles')
df['date'] = df['datetime'].dt.date
df['year'] = df['datetime'].dt.year
df['month'] = df['datetime'].dt.month
df['day'] = df['datetime'].dt.day
df['hour'] = df['datetime'].dt.hour
df['day_of_week'] = df['datetime'].dt.day_name()
df['week_number'] = df['datetime'].dt.isocalendar().week

# Extract domain from URL
df['domain'] = df['url'].apply(lambda x: urlparse(x).netloc)

print("=" * 80)
print("INSTAPAPER BOOKMARKS ANALYSIS")
print("=" * 80)

# 1. EARLIEST AND LATEST ARTICLES
print("\n1. EARLIEST AND LATEST ARTICLES")
print("-" * 80)
earliest = df.loc[df['timestamp'].idxmin()]
latest = df.loc[df['timestamp'].idxmax()]

print(f"\n📚 Total articles: {len(df):,}")
print(f"\n🕰️  EARLIEST BOOKMARK:")
print(f"   Date: {earliest['datetime']}")
print(f"   Title: {earliest['title'][:80]}")
print(f"   URL: {earliest['url'][:80]}")

print(f"\n🆕 LATEST BOOKMARK:")
print(f"   Date: {latest['datetime']}")
print(f"   Title: {latest['title'][:80]}")
print(f"   URL: {latest['url'][:80]}")

print(f"\n📅 Date range: {(latest['datetime'] - earliest['datetime']).days} days")
print(f"   ({earliest['datetime'].strftime('%Y-%m-%d')} to {latest['datetime'].strftime('%Y-%m-%d')})")

# 2. GITHUB-STYLE ACTIVITY CALENDAR
print("\n\n2. CREATING GITHUB-STYLE ACTIVITY CALENDAR")
print("-" * 80)

# Count bookmarks per day - keep as date objects
daily_counts_raw = df.groupby('date').size().reset_index(name='count')

# Create a complete date range
date_range = pd.date_range(start=df['datetime'].min(), end=df['datetime'].max(), freq='D', tz='America/Los_Angeles')
all_dates = pd.DataFrame({'date': date_range})
all_dates['date_only'] = all_dates['date'].dt.date

# Merge on date objects
daily_counts = all_dates.merge(daily_counts_raw, left_on='date_only', right_on='date', how='left').fillna(0)
daily_counts = daily_counts[['date_x', 'count']].rename(columns={'date_x': 'date'})
daily_counts['count'] = daily_counts['count'].astype(int)

# Add calendar info
daily_counts['year'] = daily_counts['date'].dt.year
daily_counts['week'] = daily_counts['date'].dt.isocalendar().week
daily_counts['day_of_week'] = daily_counts['date'].dt.dayofweek

# Create visualization for last 365 days
import pytz
pst = pytz.timezone('America/Los_Angeles')
now_pst = datetime.now(pst)
last_365_days = daily_counts[daily_counts['date'] >= (now_pst - timedelta(days=365))]
last_365_days['week_adj'] = ((last_365_days['date'] - last_365_days['date'].min()).dt.days // 7)

fig, ax = plt.subplots(figsize=(20, 6))

# Prepare data for heatmap
pivot_data = last_365_days.pivot_table(
    values='count',
    index='day_of_week',
    columns='week_adj',
    fill_value=0
)

# Calculate percentile-based color thresholds (like GitHub)
max_count = int(pivot_data.max().max())
counts_flat = pivot_data.values.flatten()
counts_nonzero = counts_flat[counts_flat > 0]

if len(counts_nonzero) > 0 and max_count > 0:
    # Define thresholds based on quartiles of non-zero values
    q1 = int(np.percentile(counts_nonzero, 25))
    q2 = int(np.percentile(counts_nonzero, 50))
    q3 = int(np.percentile(counts_nonzero, 75))

    # Create discrete color levels like GitHub
    from matplotlib.colors import ListedColormap, BoundaryNorm

    # Use actual GitHub-like green colors
    colors = ['#ebedf0', '#9be9a8', '#40c463', '#30a14e', '#216e39']
    boundaries = [0, 0.5, q1, q2, q3, max_count + 1]
    cmap = ListedColormap(colors)
    norm = BoundaryNorm(boundaries, len(colors))

    # Create heatmap with discrete colors using imshow for better control
    im = ax.imshow(pivot_data, cmap=cmap, norm=norm, aspect='auto', interpolation='nearest')

    # Style
    ax.set_yticks(range(7))
    ax.set_yticklabels(['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'])
    ax.set_xlabel('Week', fontsize=12)
    ax.set_ylabel('Day of Week', fontsize=12)
    ax.tick_params(which='major', bottom=False, left=False)

    # Add colorbar with meaningful ticks
    cbar = plt.colorbar(im, ax=ax, label='Bookmarks per day',
                       boundaries=boundaries,
                       ticks=[0, q1, q2, q3, max_count])
    cbar.ax.set_yticklabels(['0', str(q1), str(q2), str(q3), str(max_count)])
else:
    # Fallback if no data
    sns.heatmap(pivot_data, cmap='Greens', linewidths=1, linecolor='white',
                cbar_kws={'label': 'Bookmarks per day'},
                yticklabels=['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'],
                ax=ax, square=True)

plt.title('GitHub-Style Activity Calendar (Last 365 Days)', fontsize=16, pad=20)
plt.xlabel('Week of Year', fontsize=12)
plt.ylabel('Day of Week', fontsize=12)
plt.tight_layout()
plt.savefig('activity_calendar.png', dpi=300, bbox_inches='tight')
print("✅ Saved: activity_calendar.png")

# 3. BOOKMARK PATTERNS ANALYSIS
print("\n\n3. BOOKMARK PATTERNS ANALYSIS")
print("-" * 80)

# Time of day pattern
print("\n⏰ TIME OF DAY PATTERN:")
hourly_counts = df.groupby('hour').size().sort_index()
print("\nBookmarks by hour:")
for hour, count in hourly_counts.items():
    bar = '█' * int(count / hourly_counts.max() * 50)
    print(f"  {hour:02d}:00 | {bar} {count}")

peak_hour = hourly_counts.idxmax()
print(f"\n🔥 Peak hour: {peak_hour}:00 ({hourly_counts.max()} bookmarks)")

# Day of week pattern
print("\n\n📆 DAY OF WEEK PATTERN:")
day_order = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday']
dow_counts = df['day_of_week'].value_counts().reindex(day_order)
print("\nBookmarks by day:")
for day, count in dow_counts.items():
    bar = '█' * int(count / dow_counts.max() * 50)
    print(f"  {day:9s} | {bar} {count}")

print(f"\n🔥 Most active day: {dow_counts.idxmax()} ({dow_counts.max()} bookmarks)")

# Monthly trend
print("\n\n📈 MONTHLY TREND:")
monthly = df.groupby([df['datetime'].dt.to_period('M')]).size()
print(f"\nAverage bookmarks per month: {monthly.mean():.1f}")
print(f"Peak month: {monthly.idxmax()} ({monthly.max()} bookmarks)")
print(f"Quiet month: {monthly.idxmin()} ({monthly.min()} bookmarks)")

# Recent trend (last 6 months)
last_6_months = df[df['datetime'] >= (now_pst - timedelta(days=180))]
recent_monthly = last_6_months.groupby(last_6_months['datetime'].dt.to_period('M')).size()
if len(recent_monthly) >= 2:
    trend = "increasing" if recent_monthly.iloc[-1] > recent_monthly.iloc[0] else "decreasing"
    print(f"Recent trend (last 6 months): {trend}")

# Seasonality (by quarter)
print("\n\n🍂 SEASONALITY:")
df['quarter'] = df['datetime'].dt.quarter
quarter_counts = df['quarter'].value_counts().sort_index()
quarters = {1: 'Q1 (Jan-Mar)', 2: 'Q2 (Apr-Jun)', 3: 'Q3 (Jul-Sep)', 4: 'Q4 (Oct-Dec)'}
print("\nBookmarks by quarter:")
for q, count in quarter_counts.items():
    bar = '█' * int(count / quarter_counts.max() * 50)
    print(f"  {quarters[q]} | {bar} {count}")

# Create comprehensive visualizations
fig, axes = plt.subplots(2, 2, figsize=(16, 12))

# 1. Time of day heatmap
axes[0, 0].bar(hourly_counts.index, hourly_counts.values, color='steelblue')
axes[0, 0].set_xlabel('Hour of Day')
axes[0, 0].set_ylabel('Number of Bookmarks')
axes[0, 0].set_title('Bookmarks by Time of Day')
axes[0, 0].grid(axis='y', alpha=0.3)

# 2. Day of week
axes[0, 1].bar(range(7), dow_counts.values, color='coral')
axes[0, 1].set_xticks(range(7))
axes[0, 1].set_xticklabels(['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'])
axes[0, 1].set_xlabel('Day of Week')
axes[0, 1].set_ylabel('Number of Bookmarks')
axes[0, 1].set_title('Bookmarks by Day of Week')
axes[0, 1].grid(axis='y', alpha=0.3)

# 3. Monthly trend over time
monthly_df = df.groupby(df['datetime'].dt.to_period('M')).size().reset_index()
monthly_df.columns = ['month', 'count']
monthly_df['month'] = monthly_df['month'].astype(str)
axes[1, 0].plot(range(len(monthly_df)), monthly_df['count'], marker='o', linewidth=2, markersize=4)
axes[1, 0].set_xlabel('Month')
axes[1, 0].set_ylabel('Number of Bookmarks')
axes[1, 0].set_title('Monthly Trend Over Time')
axes[1, 0].grid(alpha=0.3)
axes[1, 0].tick_params(axis='x', rotation=45)

# 4. Top domains
top_domains = df['domain'].value_counts().head(15)
axes[1, 1].barh(range(len(top_domains)), top_domains.values, color='mediumseagreen')
axes[1, 1].set_yticks(range(len(top_domains)))
axes[1, 1].set_yticklabels(top_domains.index, fontsize=8)
axes[1, 1].set_xlabel('Number of Bookmarks')
axes[1, 1].set_title('Top 15 Domains')
axes[1, 1].invert_yaxis()
axes[1, 1].grid(axis='x', alpha=0.3)

plt.tight_layout()
plt.savefig('bookmark_patterns.png', dpi=300, bbox_inches='tight')
print("\n✅ Saved: bookmark_patterns.png")

# 4. CATEGORIZATION SUGGESTIONS
print("\n\n4. CATEGORIZATION SUGGESTIONS")
print("-" * 80)

# Analyze domains
print("\n🌐 TOP DOMAINS:")
top_20_domains = df['domain'].value_counts().head(20)
for i, (domain, count) in enumerate(top_20_domains.items(), 1):
    print(f"  {i:2d}. {domain:40s} {count:4d} bookmarks")

# Analyze by existing folders
print("\n\n📁 EXISTING FOLDERS:")
folder_counts = df['folder'].value_counts()
for folder, count in folder_counts.items():
    print(f"  {folder}: {count} bookmarks")

# Word frequency in titles (for topic modeling)
print("\n\n🔤 COMMON WORDS IN TITLES (potential categories):")
from collections import Counter
import re

all_titles = ' '.join(df['title'].dropna().astype(str))
# Remove common words and extract meaningful terms
words = re.findall(r'\b[a-zA-Z]{4,}\b', all_titles.lower())
stop_words = {'with', 'from', 'that', 'this', 'your', 'github', 'page',
              'home', 'about', 'blog', 'news', 'site', 'article'}
filtered_words = [w for w in words if w not in stop_words]
word_freq = Counter(filtered_words).most_common(30)

print("\nMost common terms:")
for word, count in word_freq[:20]:
    bar = '█' * int(count / word_freq[0][1] * 30)
    print(f"  {word:15s} | {bar} {count}")

# Categorization suggestions
print("\n\n💡 CATEGORIZATION SUGGESTIONS:")
print("\n1. BY SOURCE TYPE:")
print("   - GitHub repositories (github.com)")
print("   - Blog posts (medium.com, substack.com, personal blogs)")
print("   - News articles (news sites)")
print("   - Documentation (docs.*, official documentation sites)")
print("   - Social media (twitter.com, reddit.com)")
print("   - Academic (arxiv.org, papers)")
print("   - Videos (youtube.com, vimeo.com)")

print("\n2. BY DOMAIN/INTEREST:")
print("   - Extract common keywords from titles")
print("   - Use top domains as categories")
print("   - Cluster by similar domain types")

print("\n3. BY TIME-BASED:")
print("   - Weekly digests")
print("   - Monthly archives")
print("   - Seasonal collections")

print("\n4. BY READING STATUS:")
print("   - Current folder structure")
print("   - Add 'priority' tags for important articles")
print("   - 'Quick reads' vs 'Long reads' based on word_count")

# Word count analysis
print("\n\n📊 ARTICLE LENGTH ANALYSIS:")
valid_wc = df[df['word_count'] > 0]['word_count']
if len(valid_wc) > 0:
    print(f"  Articles with word count: {len(valid_wc)}")
    print(f"  Average length: {valid_wc.mean():.0f} words")
    print(f"  Median length: {valid_wc.median():.0f} words")
    print(f"  Shortest: {valid_wc.min():.0f} words")
    print(f"  Longest: {valid_wc.max():.0f} words")

    print("\n  Suggested length categories:")
    short = len(valid_wc[valid_wc <= 500])
    medium = len(valid_wc[(valid_wc > 500) & (valid_wc <= 2000)])
    long = len(valid_wc[valid_wc > 2000])
    print(f"    Quick reads (≤500 words): {short} articles")
    print(f"    Medium reads (501-2000 words): {medium} articles")
    print(f"    Long reads (>2000 words): {long} articles")

print("\n\n5. MACHINE LEARNING APPROACH:")
print("   - Use content_preview for text clustering (k-means, DBSCAN)")
print("   - Topic modeling with LDA or NMF")
print("   - Embedding-based clustering with sentence transformers")
print("   - URL pattern analysis for automatic categorization")

print("\n" + "=" * 80)
print("ANALYSIS COMPLETE!")
print("=" * 80)
print("\nGenerated files:")
print("  - activity_calendar.png")
print("  - bookmark_patterns.png")
