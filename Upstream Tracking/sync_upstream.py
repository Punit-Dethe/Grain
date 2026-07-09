import urllib.request
import json
import os
from datetime import datetime
import re

API_URL = "https://api.github.com/repos/cjpais/handy/commits?per_page=30"
# Script runs from root via GitHub Actions, or from Upstream Tracking folder locally
# Determine path to data.json
script_dir = os.path.dirname(os.path.abspath(__file__))
DATA_FILE = os.path.join(script_dir, "data.json")

def fetch_upstream_commits():
    req = urllib.request.Request(API_URL)
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
        
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode())
    except Exception as e:
        print(f"Error fetching from GitHub API: {e}")
        return []

def update_data():
    try:
        with open(DATA_FILE, 'r', encoding='utf-8') as f:
            data = json.load(f)
    except FileNotFoundError:
        print(f"Could not find {DATA_FILE}. Starting fresh.")
        data = []
        
    def normalize(msg):
        # Remove PR numbers and backticks for robust matching
        clean = re.sub(r'\(#\d+\)', '', msg)
        clean = clean.replace('`', '')
        return clean.strip().lower()
        
    existing_normalized = {normalize(item['commit']) for item in data}
    
    new_commits = fetch_upstream_commits()
    added_count = 0
    
    for commit_obj in reversed(new_commits): # Reverse to add oldest first from the page
        msg = commit_obj['commit']['message'].split('\n')[0]
        date_str = commit_obj['commit']['committer']['date']
        
        # Parse date to 'Jul 09, 2026' format
        dt = datetime.strptime(date_str, "%Y-%m-%dT%H:%M:%SZ")
        formatted_date = dt.strftime("%b %d, %Y")
        
        norm_msg = normalize(msg)
        
        if norm_msg not in existing_normalized:
            pr_match = re.search(r'\(#(\d+)\)', msg)
            pr_num = pr_match.group(1) if pr_match else ""
            
            data.append({
                'date': formatted_date,
                'commit': msg,
                'status': 'Pending',
                'notes': '',
                'pr': pr_num
            })
            existing_normalized.add(norm_msg)
            added_count += 1
            
    if added_count > 0:
        # Re-sort data by date descending just to be safe
        data.sort(key=lambda x: datetime.strptime(x['date'], "%b %d, %Y"), reverse=True)
        with open(DATA_FILE, 'w', encoding='utf-8') as f:
            json.dump(data, f, indent=2)
        print(f"Added {added_count} new commits to {DATA_FILE}.")
    else:
        print("No new commits found.")

if __name__ == "__main__":
    update_data()
