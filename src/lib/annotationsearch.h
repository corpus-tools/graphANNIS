#ifndef ANNOTATIONSEARCH_H
#define ANNOTATIONSEARCH_H

#include "db.h"
#include "annotationiterator.h"

namespace annis
{

class AnnotationNameSearch : public CacheableAnnoIt
{
typedef stx::btree_multimap<Annotation, nodeid_t, compAnno>::const_iterator ItType;

public:
  /**
   * @brief Find all annotations.
   * @param db
   */
  AnnotationNameSearch(DB& db);
  /**
   * @brief Find annotations by name
   * @param db
   * @param annoName
   */
  AnnotationNameSearch(const DB& db, const std::string& annoName);
  AnnotationNameSearch(const DB& db, const std::string& annoNamspace, const std::string& annoName);
  AnnotationNameSearch(const DB &db, const std::string& annoNamspace, const std::string& annoName, const std::string& annoValue);

  virtual ~AnnotationNameSearch();

  virtual bool hasNext()
  {
    return it != db.inverseNodeAnnotations.end() && it != itEnd;
  }
  virtual Match next();
  virtual Match current();
  virtual void reset();

  const Annotation& getAnnotation() {return anno;}

private:
  const DB& db;

  ItType it;
  ItType itBegin;
  ItType itEnd;

  Annotation anno;

  bool currentMatchValid;
  Match currentMatch;

};
} // end namespace annis
#endif // ANNOTATIONSEARCH_H
