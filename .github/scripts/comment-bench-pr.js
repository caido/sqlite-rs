import fs from "fs";

export default async function comment({ github, context }) {
  const marker = "<!-- criterion-bench-compare -->";
  const raw = fs.readFileSync("bench-compare.txt", "utf8");
  const clipped = raw.length > 60000 ? raw.slice(-60000) : raw;
  const body = `${marker}\n## Bench vs main\n\n\`\`\`\n${clipped}\n\`\`\`\n`;

  const { data: comments } = await github.rest.issues.listComments({
    owner: context.repo.owner,
    repo: context.repo.repo,
    issue_number: context.issue.number,
  });

  const existing = comments.find(
    (c) => c.user.type === "Bot" && c.body.includes(marker),
  );

  if (existing) {
    await github.rest.issues.updateComment({
      owner: context.repo.owner,
      repo: context.repo.repo,
      comment_id: existing.id,
      body,
    });
  } else {
    await github.rest.issues.createComment({
      owner: context.repo.owner,
      repo: context.repo.repo,
      issue_number: context.issue.number,
      body,
    });
  }
};
