function zoneRecordEditor() {
    const modalEl = document.getElementById("zoneRecordEditModal");
    if (!modalEl) {
        return;
    }

    const form = document.getElementById("zoneRecordEditForm");
    const errorEl = document.getElementById("zoneRecordEditError");
    const saveButton = document.getElementById("zoneRecordEditSave");
    const fields = {
        id: document.getElementById("zoneRecordEditId"),
        name: document.getElementById("zoneRecordEditName"),
        rrtype: document.getElementById("zoneRecordEditType"),
        rclass: document.getElementById("zoneRecordEditClass"),
        ttl: document.getElementById("zoneRecordEditTtl"),
        rdata: document.getElementById("zoneRecordEditData"),
    };

    const modal = new bootstrap.Modal(modalEl);

    const openEditor = (row) => {
        fields.id.value = row.dataset.recordId || "";
        fields.name.value = row.dataset.recordName || "";
        fields.rrtype.value = row.dataset.recordRrtype || "";
        fields.rclass.value = row.dataset.recordRclass || "";
        fields.ttl.value = row.dataset.recordTtl || "";
        fields.rdata.value = row.dataset.recordRdata || "";
        errorEl.classList.add("d-none");
        errorEl.textContent = "";
        modal.show();
    };

    document.querySelectorAll("[data-record-edit]").forEach((row) => {
        row.addEventListener("click", (event) => {
            if (event.target.closest("button, a, input, select, textarea")) {
                openEditor(row);
                return;
            }
            openEditor(row);
        });

        row.addEventListener("keydown", (event) => {
            if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                openEditor(row);
            }
        });
    });

    form.addEventListener("submit", async (event) => {
        event.preventDefault();
        saveButton.disabled = true;
        errorEl.classList.add("d-none");
        errorEl.textContent = "";

        const payload = {
            id: fields.id.value,
            name: fields.name.value.trim(),
            rdata: fields.rdata.value.trim(),
        };

        const rrtype = Number.parseInt(fields.rrtype.value, 10);
        if (!Number.isNaN(rrtype)) {
            payload.rrtype = rrtype;
        }

        const rclass = Number.parseInt(fields.rclass.value, 10);
        if (!Number.isNaN(rclass)) {
            payload.rclass = rclass;
        }

        const ttlValue = fields.ttl.value.trim();
        if (ttlValue !== "") {
            const ttl = Number.parseInt(ttlValue, 10);
            if (!Number.isNaN(ttl)) {
                payload.ttl = ttl;
            }
        }

        try {
            const response = await fetch("/api/record", {
                method: "PUT",
                headers: {
                    "Content-Type": "application/json",
                },
                body: JSON.stringify(payload),
            });

            if (!response.ok) {
                const errorPayload = await response.json().catch(() => ({}));
                const message =
                    errorPayload.error ||
                    errorPayload.message ||
                    "Failed to update record.";
                throw new Error(message);
            }

            window.location.reload();
        } catch (error) {
            errorEl.textContent = error.message || "Failed to update record.";
            errorEl.classList.remove("d-none");
        } finally {
            saveButton.disabled = false;
        }
    });
}

document.addEventListener("DOMContentLoaded", () => {
    zoneRecordEditor();
});
