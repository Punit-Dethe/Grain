let allData = [];
let currentView = 'Overview'; // Overview, Pending, Merged, Ignored

document.addEventListener('DOMContentLoaded', () => {
    fetchData();
    
    // Search handler
    document.getElementById('searchInput').addEventListener('input', renderList);
    
    // Navigation handlers
    document.querySelectorAll('.nav-item').forEach(btn => {
        btn.addEventListener('click', (e) => {
            document.querySelectorAll('.nav-item').forEach(b => b.classList.remove('active'));
            const target = e.currentTarget;
            target.classList.add('active');
            
            currentView = target.dataset.view;
            document.getElementById('viewTitle').innerText = currentView;
            
            if (currentView === 'Overview') {
                document.getElementById('overviewStats').style.display = 'grid';
                document.getElementById('listTitle').innerText = 'Recent Commits';
            } else {
                document.getElementById('overviewStats').style.display = 'none';
                document.getElementById('listTitle').innerText = `All ${currentView}`;
            }
            
            // Clear search when changing views
            document.getElementById('searchInput').value = '';
            renderList();
        });
    });
});

async function fetchData() {
    try {
        const response = await fetch('data.json');
        allData = await response.json();
        
        allData.sort((a, b) => new Date(b.date) - new Date(a.date));
        
        updateStats();
        renderList();
    } catch (error) {
        console.error('Error fetching data:', error);
        document.getElementById('commitList').innerHTML = `
            <div class="empty-state">Failed to load tracking data.</div>
        `;
    }
}

function updateStats() {
    const pending = allData.filter(d => d.status === 'Pending').length;
    const merged = allData.filter(d => d.status === 'Merged').length;
    const ignored = allData.filter(d => d.status === 'Ignored').length;
    
    document.getElementById('stat-total').innerText = allData.length;
    document.getElementById('stat-pending').innerText = pending;
    document.getElementById('stat-merged').innerText = merged;
    
    document.getElementById('count-pending').innerText = pending;
    document.getElementById('count-merged').innerText = merged;
    document.getElementById('count-ignored').innerText = ignored;
    
    if (allData.length > 0) {
        document.getElementById('lastUpdated').innerText = `Last update: ${allData[0].date}`;
    }
}

function renderList() {
    const searchQuery = document.getElementById('searchInput').value.toLowerCase();
    
    let filtered = allData.filter(item => {
        const matchesSearch = item.commit.toLowerCase().includes(searchQuery) || 
                              (item.notes && item.notes.toLowerCase().includes(searchQuery));
        
        let matchesStatus = true;
        if (currentView !== 'Overview') {
            matchesStatus = item.status === currentView;
        }
        
        return matchesSearch && matchesStatus;
    });
    
    // In Overview, if not searching, limit to 15 recent items
    if (currentView === 'Overview' && searchQuery === '') {
        filtered = filtered.slice(0, 15);
    }
    
    const container = document.getElementById('commitList');
    container.innerHTML = '';
    
    if (filtered.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                No commits found in this view.
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
            if (item.notes !== item.status) {
                notesHtml = `<div class="commit-notes">${item.notes}</div>`;
            }
        }
        
        let commitText = item.commit;
        if (item.pr) {
            commitText = commitText.replace(`(#${item.pr})`, `<strong>(#${item.pr})</strong>`);
        }
        
        el.innerHTML = `
            <div class="commit-header">
                <div class="status-dot ${badgeClass}"></div>
                <div class="commit-main">
                    <div class="commit-title">${commitText}</div>
                    <div class="commit-meta">${item.date} • Status: ${item.status}</div>
                </div>
            </div>
            ${notesHtml}
        `;
        
        container.appendChild(el);
    });
}
