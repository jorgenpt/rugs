UPDATE projects
    SET
	      stream = concat(stream, '/', rtrim(rtrim(project, replace(project, rtrim(project, replace(project, '/', '')), '')), '/')),
	      project = replace(project, rtrim(project, replace(project, '/', '')), '')
    WHERE project LIKE '%/%';
