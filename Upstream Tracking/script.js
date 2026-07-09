let allData = [];

document.addEventListener('DOMContentLoaded', () => {
    fetchData();
    
    document.getElementById('searchInput').addEventListener('input', renderList);
    document.getElementById('statusFilter').addEventListener('change', renderList);
});

async function fetchData() {
    try {
        const response = await fetch('data.json');
        allData = await response.json();
        
        // Sort by date descending
        allData.sort((a, b) => new Date(b.date) - new Date(a.date));
        
        updateStats();
        renderList();
    } catch (error) {
        console.error('Error fetching data:', error);
        document.getElementById('commitList').innerHTML = `
            <div class="empty-state">
                Failed to load tracking data. Make sure data.json exists and is valid.
            </div>
        `;
    }
}

function updateStats() {
    document.getElementById('totalCount').innerText = allData.length;
    document.getElementById('pendingCount').innerText = allData.filter(d => d.status === 'Pending').length;
    document.getElementById('mergedCount').innerText = allData.filter(d => d.status === 'Merged').length;
}

function renderList() {
    const searchQuery = document.getElementById('searchInput').value.toLowerCase();
    const statusFilter = document.getElementById('statusFilter').value;
    
    const filtered = allData.filter(item => {
        const matchesSearch = item.commit.toLowerCase().includes(searchQuery) || 
                              (item.notes && item.notes.toLowerCase().includes(searchQuery));
        const matchesStatus = statusFilter === 'All' || item.status === statusFilter;
        return matchesSearch && matchesStatus;
    });
    
    const container = document.getElementById('commitList');
    container.innerHTML = '';
    
    if (filtered.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                No updates found matching your filters.
            </div>
        `;
        return;
    }
    
    filtered.forEach(item => {
        const el = document.createElement('div');
        el.className = 'commit-card';
        
        const badgeClass = item.status.toLowerCase();
        
        let notesHtml = '';
        if (item.notes && item.notes.trim() !== 'Pending' && item.notes.trim() !== '') {
            // Avoid just printing "Pending" or "Safely merged into tray.rs" if it's redundant, but usually we want to print it
            if (item.notes !== item.status) {
                notesHtml = `<div class="commit-notes">${item.notes}</div>`;
            }
        }
        
        // Format commit text to bold PR numbers if they exist
        let commitText = item.commit;
        if (item.pr) {
            commitText = commitText.replace(`(#${item.pr})`, `<strong>(#${item.pr})</strong>`);
        }
        
        el.innerHTML = `
            <div class="commit-header">
                <div>
                    <div class="commit-title">${commitText}</div>
                    <div class="commit-meta">
                        <span>${item.date}</span>
                    </div>
                </div>
                <div class="badge ${badgeClass}">${item.status}</div>
            </div>
            ${notesHtml}
        `;
        
        container.appendChild(el);
    });
}
